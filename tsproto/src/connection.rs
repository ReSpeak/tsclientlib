use std::cell::RefCell;
use std::net::SocketAddr;
use std::rc::{Rc, Weak};
use std::u16;

use {slog, tomcrypt};
use futures::{self, Sink, Stream};
use num::ToPrimitive;

use {Error, StreamWrapper, SinkWrapper};
use connectionmanager::ConnectionManager;
use packets::*;
use handler_data::Data;

/// Data that has to be stored for a connection when it is connected.
#[derive(Debug)]
pub struct ConnectedParams {
    /// The next packet id that should be sent.
    ///
    /// This list is indexed by the [`PacketType`], [`PacketType::Init`] is an
    /// invalid index.
    ///
    /// [`PacketType`]: udp/enum.PacketType.html
    /// [`PacketType::Init`]: udp/enum.PacketType.html
    pub outgoing_p_ids: [(u32, u16); 8],
    /// Used for incoming out-of-order packets.
    ///
    /// Only used for `Command` and `CommandLow` packets.
    pub receive_queue: [Vec<(Header, Vec<u8>)>; 2],
    /// Used for incoming fragmented packets.
    ///
    /// Only used for `Command` and `CommandLow` packets.
    pub fragmented_queue: [Option<(Header, Vec<u8>)>; 2],
    /// The next packet id that is expected.
    ///
    /// Works like the `outgoing_p_ids`.
    pub incoming_p_ids: [(u32, u16); 8],

    /// The client id of this connection.
    pub c_id: u16,
    /// If voice packets should be encrypted
    pub voice_encryption: bool,

    pub public_key: tomcrypt::EccKey,
    /// The iv used to encrypt and decrypt packets.
    pub shared_iv: [u8; 20],
    /// The mac used for unencrypted packets.
    pub shared_mac: [u8; 8],
}

impl ConnectedParams {
    /// Fills the parameters for a connection with their default state.
    pub fn new(public_key: tomcrypt::EccKey, shared_iv: [u8; 20], shared_mac: [u8; 8]) -> Self {
        Self {
            outgoing_p_ids: Default::default(),
            receive_queue: Default::default(),
            fragmented_queue: Default::default(),
            incoming_p_ids: Default::default(),
            c_id: 0,
            voice_encryption: true,
            public_key,
            shared_iv,
            shared_mac,
        }
    }

    /// Check if a given id is in the receive window.
    pub(crate) fn in_receive_window(
        &self,
        p_type: PacketType,
        p_id: u16,
    ) -> (bool, u16, u16) {
        let type_i = p_type.to_usize().unwrap();
        // Receive window is the next half of ids
        let cur_next = self.incoming_p_ids[type_i].1;
        let limit = ((u32::from(cur_next) + u32::from(u16::MAX) / 2)
            % u32::from(u16::MAX)) as u16;
        (
            (cur_next < limit && p_id >= cur_next && p_id < limit)
                || (cur_next > limit && (p_id >= cur_next || p_id < limit)),
            cur_next,
            limit,
        )
    }
}

/// Represents a currently alive connection.
pub struct Connection<CM: ConnectionManager> {
    /// A logger for this connection.
    pub logger: slog::Logger,
    /// If this is the connection stored in a client or in a server.
    pub is_client: bool,
    /// The parameters of this connection, if it is already established.
    pub params: Option<ConnectedParams>,
    /// The adress of the other side, where packets are coming from and going
    /// to.
    pub address: SocketAddr,

    pub(crate) udp_packet_buffer_stream: ::BufferStream<UdpPacket, Error>,

    /// The stream for [`UdpPacket`]s.
    ///
    /// [`UdpPacket`]: ../packets/struct.UdpPacket.html
    pub udp_packet_stream:
        Option<Box<Stream<Item = UdpPacket, Error = Error>>>,
    /// The sink for [`UdpPacket`]s.
    ///
    /// [`UdpPacket`]: ../packets/struct.UdpPacket.html
    pub udp_packet_sink:
        Option<Box<Sink<SinkItem = UdpPacket, SinkError = Error>>>,
    /// The stream for [`Packet`]s.
    ///
    /// [`Packet`]: ../packets/struct.Packet.html
    pub packet_stream:
        Option<Box<Stream<Item = Packet, Error = Error>>>,
    /// The sink for [`Packet`]s.
    ///
    /// [`Packet`]: ../packets/struct.Packet.html
    pub packet_sink:
        Option<Box<Sink<SinkItem = Packet, SinkError = Error>>>,

    /// For `Command` and `CommandLow` packets.
    pub(crate) command_buffer_stream: ::BufferStream<Packet, Error>,
    /// For `Voice` packets.
    pub(crate) voice_buffer_stream: ::BufferStream<Packet, Error>,
    /// For `VoiceWhisper` packets.
    pub(crate) voice_whisper_buffer_stream: ::BufferStream<Packet, Error>,

    pub resender: CM::Resend,
}

impl<CM: ConnectionManager + 'static> Connection<CM> {
    /// Creates a new connection struct.
    pub fn new(data: Rc<RefCell<Data<CM>>>, address: SocketAddr,
        resender: CM::Resend) -> Rc<RefCell<Self>> {
        let (logger, is_client) = {
            let data = data.borrow();
            (data.logger.clone(), data.is_client)
        };

        let con = Rc::new(RefCell::new(Self {
            logger,
            is_client,
            params: None,
            address,

            udp_packet_buffer_stream: Default::default(),
            udp_packet_stream: None,
            udp_packet_sink: None,
            packet_stream: None,
            packet_sink: None,

            command_buffer_stream: Default::default(),
            voice_buffer_stream: Default::default(),
            voice_whisper_buffer_stream: Default::default(),

            resender,
        }));

        // Set the udp stream and sink
        let stream = ConnectionUdpPacketStream::new(con.clone());
        con.borrow_mut().udp_packet_stream = Some(Box::new(stream));

        let data_packets = Data::get_udp_packets(data.clone());
        let sink = ConnectionUdpPacketSink::new(data_packets, con.clone());
        con.borrow_mut().udp_packet_sink = Some(Box::new(sink));

        con
    }

    pub fn apply_udp_packet_stream_wrapper<
        W: StreamWrapper<UdpPacket, Error,
            Box<Stream<Item = UdpPacket, Error = Error>>>
            + 'static,
    >(connection: Rc<RefCell<Self>>, a: W::A) {
        let mut connection = connection.borrow_mut();
        let inner = connection.udp_packet_stream.take().unwrap();
        connection.udp_packet_stream = Some(Box::new(W::wrap(inner, a)));
    }

    pub fn apply_udp_packet_sink_wrapper<
        W: SinkWrapper<UdpPacket, Error,
            Box<Sink<SinkItem = UdpPacket, SinkError = Error>>>
            + 'static,
    >(connection: Rc<RefCell<Self>>, a: W::A) {
        let mut connection = connection.borrow_mut();
        let inner = connection.udp_packet_sink.take().unwrap();
        connection.udp_packet_sink = Some(Box::new(W::wrap(inner, a)));
    }

    pub fn apply_packet_stream_wrapper<
        W: StreamWrapper<Packet, Error,
            Box<Stream<Item = Packet, Error = Error>>>
            + 'static,
    >(connection: Rc<RefCell<Self>>, a: W::A) {
        let mut connection = connection.borrow_mut();
        let inner = connection.packet_stream.take().unwrap();
        connection.packet_stream = Some(Box::new(W::wrap(inner, a)));
    }

    pub fn apply_packet_sink_wrapper<
        W: SinkWrapper<Packet, Error,
            Box<Sink<SinkItem = Packet, SinkError = Error>>>
            + 'static,
    >(connection: Rc<RefCell<Self>>, a: W::A) {
        let mut connection = connection.borrow_mut();
        let inner = connection.packet_sink.take().unwrap();
        connection.packet_sink = Some(Box::new(W::wrap(inner, a)));
    }

    /// Gives a `Stream` and `Sink` of [`UdpPacket`]s, which always references the
    /// current stream in the `Connection` struct.
    pub fn get_udp_packets(connection: Rc<RefCell<Self>>) -> UdpPackets<CM> {
        UdpPackets {
            connection: Rc::downgrade(&connection),
        }
    }

    /// Gives a `Stream` and `Sink` of [`Packet`]s, which always references the
    /// current stream in the `Connection` struct.
    ///
    /// [`Packet`]: ../packets/struct.Packet.html
    pub fn get_packets(connection: Rc<RefCell<Self>>) -> Packets<CM> {
        Packets {
            connection: Rc::downgrade(&connection),
        }
    }

    /// Returns a stream of all `Command` and `CommandLow` packets that arrive
    /// for this connection.
    pub fn get_commands(connection: Rc<RefCell<Self>>)
        -> ConnectionCommandPacketStream<CM> {
        ConnectionCommandPacketStream::new(connection)
    }

    /// Returns a stream of all `Voice` packets that arrive for this connection.
    pub fn get_voice(connection: Rc<RefCell<Self>>)
        -> ConnectionVoicePacketStream<CM> {
        ConnectionVoicePacketStream::new(connection)
    }

    /// Returns a stream of all `VoiceWhisper` packets that arrive for this
    /// connection.
    pub fn get_voice_whisper(connection: Rc<RefCell<Self>>)
        -> ConnectionVoiceWhisperPacketStream<CM> {
        ConnectionVoiceWhisperPacketStream::new(connection)
    }
}

/// A `Stream` and `Sink` of [`UdpPacket`]s, which always references the current
/// stream in the [`Connection`] struct.
///
/// [`UdpPacket`]: ../packets/struct.UdpPacket.html
/// [`Connection`]: struct.Connection.html
pub struct UdpPackets<CM: ConnectionManager> {
    connection: Weak<RefCell<Connection<CM>>>,
}

/// A `Stream` and `Sink` of [`Packet`]s, which always references the current
/// stream in the [`Connection`] struct.
///
/// [`Packet`]: ../packets/struct.Packet.html
/// [`Connection`]: struct.Connection.html
pub struct Packets<CM: ConnectionManager> {
    connection: Weak<RefCell<Connection<CM>>>,
}

impl<CM: ConnectionManager> Stream for UdpPackets<CM> {
    type Item = UdpPacket;
    type Error = Error;

    fn poll(&mut self) -> futures::Poll<Option<Self::Item>, Self::Error> {
        let connection = if let Some(connection) = self.connection.upgrade() {
            connection
        } else {
            return Ok(futures::Async::Ready(None));
        };
        let mut stream = {
            let mut connection = connection.borrow_mut();
            connection.udp_packet_stream
                .take()
                .unwrap()
        };
        let res = stream.poll();
        let mut connection = connection.borrow_mut();
        connection.udp_packet_stream = Some(stream);
        res
    }
}

impl<CM: ConnectionManager> Sink for UdpPackets<CM> {
    type SinkItem = UdpPacket;
    type SinkError = Error;

    fn start_send(
        &mut self,
        item: Self::SinkItem,
    ) -> futures::StartSend<Self::SinkItem, Self::SinkError> {
        let connection = self.connection.upgrade().unwrap();
        let mut sink = {
            let mut connection = connection.borrow_mut();
            connection.udp_packet_sink.take().unwrap()
        };
        let res = sink.start_send(item);
        let mut connection = connection.borrow_mut();
        connection.udp_packet_sink = Some(sink);
        res
    }

    fn poll_complete(&mut self) -> futures::Poll<(), Self::SinkError> {
        let connection = self.connection.upgrade().unwrap();
        let mut sink = {
            let mut connection = connection.borrow_mut();
            connection.udp_packet_sink
                .take()
                .unwrap()
        };
        let res = sink.poll_complete();
        let mut connection = connection.borrow_mut();
        connection.udp_packet_sink = Some(sink);
        res
    }

    fn close(&mut self) -> futures::Poll<(), Self::SinkError> {
        let connection = self.connection.upgrade().unwrap();
        let mut sink = {
            let mut connection = connection.borrow_mut();
            connection.udp_packet_sink
                .take()
                .unwrap()
        };
        let res = sink.close();
        let mut connection = connection.borrow_mut();
        connection.udp_packet_sink = Some(sink);
        res
    }
}

impl<CM: ConnectionManager> Stream for Packets<CM> {
    type Item = Packet;
    type Error = Error;

    fn poll(&mut self) -> futures::Poll<Option<Self::Item>, Self::Error> {
        let connection = if let Some(connection) = self.connection.upgrade() {
            connection
        } else {
            return Ok(futures::Async::Ready(None));
        };
        let mut stream = connection.borrow_mut()
            .packet_stream
            .take()
            .expect("Packet stream not available");
        let res = stream.poll();
        connection.borrow_mut().packet_stream = Some(stream);
        res
    }
}

impl<CM: ConnectionManager> Sink for Packets<CM> {
    type SinkItem = Packet;
    type SinkError = Error;

    fn start_send(&mut self, item: Self::SinkItem)
        -> futures::StartSend<Self::SinkItem, Self::SinkError> {
        let connection = self.connection.upgrade()
            .expect("Underlying connection was removed");
        let mut sink = connection.borrow_mut()
            .packet_sink
            .take()
            .expect("Packet sink not available");
        let res = sink.start_send(item);
        connection.borrow_mut().packet_sink = Some(sink);
        res
    }

    fn poll_complete(&mut self) -> futures::Poll<(), Self::SinkError> {
        let connection = self.connection.upgrade().unwrap();
        let mut sink = connection.borrow_mut()
            .packet_sink
            .take()
            .expect("Packet sink not available");
        let res = sink.poll_complete();
        connection.borrow_mut().packet_sink = Some(sink);
        res
    }

    fn close(&mut self) -> futures::Poll<(), Self::SinkError> {
        let connection = self.connection.upgrade().unwrap();
        let mut sink = connection.borrow_mut()
            .packet_sink
            .take()
            .expect("Packet sink not available");
        let res = sink.close();
        connection.borrow_mut().packet_sink = Some(sink);
        res
    }
}

struct ConnectionUdpPacketStream<CM: ConnectionManager> {
    connection: Weak<RefCell<Connection<CM>>>,
}
impl<CM: ConnectionManager> ConnectionUdpPacketStream<CM> {
    fn new(con: Rc<RefCell<Connection<CM>>>) -> Self {
        Self { connection: Rc::downgrade(&con) }
    }
}
impl<CM: ConnectionManager> Stream for ConnectionUdpPacketStream<CM> {
    type Item = UdpPacket;
    type Error = Error;

    fn poll(&mut self) -> futures::Poll<Option<Self::Item>, Self::Error> {
        let con = if let Some(con) = self.connection.upgrade() {
            con
        } else {
            // The connection does not exist anymore, just quit
            return Ok(futures::Async::Ready(None));
        };
        let mut con = con.borrow_mut();
        con.udp_packet_buffer_stream.poll()
    }
}

pub struct ConnectionCommandPacketStream<CM: ConnectionManager> {
    connection: Weak<RefCell<Connection<CM>>>,
}
impl<CM: ConnectionManager> ConnectionCommandPacketStream<CM> {
    fn new(con: Rc<RefCell<Connection<CM>>>) -> Self {
        Self { connection: Rc::downgrade(&con) }
    }
}
impl<CM: ConnectionManager> Stream for ConnectionCommandPacketStream<CM> {
    type Item = Packet;
    type Error = Error;

    fn poll(&mut self) -> futures::Poll<Option<Self::Item>, Self::Error> {
        let con = if let Some(con) = self.connection.upgrade() {
            con
        } else {
            // The connection does not exist anymore, just quit
            return Ok(futures::Async::Ready(None));
        };
        let mut con = con.borrow_mut();
        con.command_buffer_stream.poll()
    }
}

pub struct ConnectionVoicePacketStream<CM: ConnectionManager> {
    connection: Weak<RefCell<Connection<CM>>>,
}
impl<CM: ConnectionManager> ConnectionVoicePacketStream<CM> {
    fn new(con: Rc<RefCell<Connection<CM>>>) -> Self {
        Self { connection: Rc::downgrade(&con) }
    }
}
impl<CM: ConnectionManager> Stream for ConnectionVoicePacketStream<CM> {
    type Item = Packet;
    type Error = Error;

    fn poll(&mut self) -> futures::Poll<Option<Self::Item>, Self::Error> {
        let con = if let Some(con) = self.connection.upgrade() {
            con
        } else {
            // The connection does not exist anymore, just quit
            return Ok(futures::Async::Ready(None));
        };
        let mut con = con.borrow_mut();
        con.voice_buffer_stream.poll()
    }
}

pub struct ConnectionVoiceWhisperPacketStream<CM: ConnectionManager> {
    connection: Weak<RefCell<Connection<CM>>>,
}
impl<CM: ConnectionManager> ConnectionVoiceWhisperPacketStream<CM> {
    fn new(con: Rc<RefCell<Connection<CM>>>) -> Self {
        Self { connection: Rc::downgrade(&con) }
    }
}
impl<CM: ConnectionManager> Stream for ConnectionVoiceWhisperPacketStream<CM> {
    type Item = Packet;
    type Error = Error;

    fn poll(&mut self) -> futures::Poll<Option<Self::Item>, Self::Error> {
        let con = if let Some(con) = self.connection.upgrade() {
            con
        } else {
            // The connection does not exist anymore, just quit
            return Ok(futures::Async::Ready(None));
        };
        let mut con = con.borrow_mut();
        con.voice_whisper_buffer_stream.poll()
    }
}

/// The sink which adds the address to packets of a connection and sends them
/// to the `Data` object.
struct ConnectionUdpPacketSink<
    Inner: Sink<SinkItem = (SocketAddr, UdpPacket), SinkError = Error>,
    CM: ConnectionManager,
> {
    inner: Inner,
    connection: Weak<RefCell<Connection<CM>>>,
}

impl<
    Inner: Sink<SinkItem = (SocketAddr, UdpPacket), SinkError = Error>,
    CM: ConnectionManager,
> ConnectionUdpPacketSink<Inner, CM> {
    fn new(inner: Inner, con: Rc<RefCell<Connection<CM>>>) -> Self {
        Self {
            inner,
            connection: Rc::downgrade(&con),
        }
    }
}

impl<
    Inner: Sink<SinkItem = (SocketAddr, UdpPacket), SinkError = Error>,
    CM: ConnectionManager,
> Sink for ConnectionUdpPacketSink<Inner, CM> {
    type SinkItem = UdpPacket;
    type SinkError = Error;

    fn start_send(&mut self, item: Self::SinkItem)
        -> futures::StartSend<Self::SinkItem, Self::SinkError> {
        let addr = {
            let con = self.connection.upgrade().unwrap();
            let con = con.borrow();
            con.address
        };
        if let futures::AsyncSink::NotReady((_, item)) =
            self.inner.start_send((addr, item))? {
            Ok(futures::AsyncSink::NotReady(item))
        } else {
            Ok(futures::AsyncSink::Ready)
        }
    }

    fn poll_complete(&mut self) -> futures::Poll<(), Self::SinkError> {
        self.inner.poll_complete()
    }

    fn close(&mut self) -> futures::Poll<(), Self::SinkError> {
        self.inner.close()
    }
}
