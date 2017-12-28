use std::cell::RefCell;
use std::net::SocketAddr;
use std::rc::{Rc, Weak};
use std::u16;

use {slog, tomcrypt};
use chrono::Duration;
use futures::{self, Sink, Stream};
use num::ToPrimitive;

use {Error, StreamWrapper, SinkWrapper};
use connectionmanager::ConnectionManager;
use packets::*;

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

    /// The stream for `UdpPacket`s.
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

    pub resender: CM::Resend,
}

impl<CM: ConnectionManager> Connection<CM> {
    /// Creates a new connection struct.
    pub fn new(logger: slog::Logger, is_client: bool, address: SocketAddr,
        resender: CM::Resend) -> Self {
        Self {
            logger,
            is_client,
            params: None,
            address,

            udp_packet_stream: None,
            udp_packet_sink: None,
            packet_stream: None,
            packet_sink: None,

            resender,
        }
    }

    /* TODO
    pub fn apply_udp_packet_stream_wrapper<
        T: Stream<Item = UdpPacket, Error = Error>,
        W: StreamWrapper<UdpPacket, Error, T>,
    >(connection: Rc<RefCell<Self>>) {
        let mut connection = connection.borrow_mut();
        let inner = connection.udp_packet_stream.take().unwrap();
        connection.udp_packet_stream = Some(W::wrap(inner));
    }

    pub fn apply_udp_packet_sink_wrapper<
        T: Sink<SinkItem = UdpPacket, SinkError = Error>,
        W: SinkWrapper<UdpPacket, Error, T>,
    >(connection: Rc<RefCell<Self>>) {
        let mut connection = connection.borrow_mut();
        let inner = connection.udp_packet_sink.take().unwrap();
        connection.udp_packet_sink = Some(W::wrap(inner));
    }

    pub fn apply_packet_stream_wrapper<
        T: Stream<Item = Packet, Error = Error>,
        W: StreamWrapper<Packet, Error, T>,
    >(connection: Rc<RefCell<Self>>) {
        let mut connection = connection.borrow_mut();
        let inner = connection.packet_stream.take().unwrap();
        connection.packet_stream = Some(W::wrap(inner));
    }

    pub fn apply_packet_sink_wrapper<
        T: Sink<SinkItem = Packet, SinkError = Error>,
        W: SinkWrapper<Packet, Error, T>,
    >(connection: Rc<RefCell<Self>>) {
        let mut connection = connection.borrow_mut();
        let inner = connection.packet_sink.take().unwrap();
        connection.packet_sink = Some(W::wrap(inner));
    }*/

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
            connection.udp_packet_sink
                .take()
                .unwrap()
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

    fn start_send(
        &mut self,
        item: Self::SinkItem,
    ) -> futures::StartSend<Self::SinkItem, Self::SinkError> {
        let connection = self.connection.upgrade().unwrap();
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
