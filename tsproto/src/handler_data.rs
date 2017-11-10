use std::cell::RefCell;
use std::cmp::{Ord, Ordering};
use std::collections::BinaryHeap;
use std::net::SocketAddr;
use std::rc::{Rc, Weak};
use std::u16;

use {slog, slog_async, slog_term};
use chrono::{DateTime, Duration, Utc};
use futures::{self, Sink, Stream};
use futures::task::Task;
use num::ToPrimitive;
use slog::Drain;
use tokio_core::net::UdpSocket;
use tokio_core::reactor::Handle;

use {Error, Map, Result, TsCodec};
use packets::*;

/// A record of a packet that can be resent.
pub struct SendRecord {
    /// When this packet was sent.
    pub sent: DateTime<Utc>,
    /// The next time when the packet will be resent.
    pub next: DateTime<Utc>,
    /// How often the packet was already resent.
    pub tries: usize,
    pub p_type: PacketType,
    pub p_id: u16,
    /// The packet of this record.
    pub packet: (SocketAddr, UdpPacket),
}

impl PartialEq for SendRecord {
    fn eq(&self, other: &Self) -> bool {
        self.next == other.next
    }
}
impl Eq for SendRecord {}

impl PartialOrd for SendRecord {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(&other))
    }
}

impl Ord for SendRecord {
    fn cmp(&self, other: &Self) -> Ordering {
        // The smallest time is the most important time
        self.next.cmp(&other.next).reverse()
    }
}

/// The stored data for our server or client.
///
/// This data is stored for one socket.
pub struct Data<ConnectionState> {
    /// If this structure is owned by a client or a server.
    pub is_client: bool,
    /// The address of the socket.
    pub local_addr: SocketAddr,
    pub(crate) handle: Handle,
    pub logger: slog::Logger,

    /// The raw udp stream.
    pub raw_stream:
        Option<Box<Stream<Item = (SocketAddr, UdpPacket), Error = Error>>>,
    /// The raw udp sink.
    pub raw_sink:
        Option<Box<Sink<SinkItem = (SocketAddr, UdpPacket), SinkError = Error>>>,
    /// The stream for `UdpPacket`s.
    pub udp_packet_stream:
        Option<Box<Stream<Item = (SocketAddr, UdpPacket), Error = Error>>>,
    /// The sink for [`UdpPacket`]s.
    ///
    /// [`UdpPacket`]: ../packets/struct.UdpPacket.html
    pub udp_packet_sink:
        Option<Box<Sink<SinkItem = (SocketAddr, UdpPacket), SinkError = Error>>>,
    /// The stream for [`Packet`]s.
    ///
    /// [`Packet`]: ../packets/struct.Packet.html
    pub packet_stream:
        Option<Box<Stream<Item = (SocketAddr, Packet), Error = Error>>>,
    /// The sink for [`Packet`]s.
    ///
    /// [`Packet`]: ../packets/struct.Packet.html
    pub packet_sink:
        Option<Box<Sink<SinkItem = (SocketAddr, Packet), SinkError = Error>>>,

    /// A list of all connected clients or servers
    pub connections: Map<SocketAddr, Connection<ConnectionState>>,

    /// A queue of packets that where sent and when they were sent.
    ///
    /// Used to resend packets when they are not acknowledged in a certain time.
    /// That is only done for `Command` and `CommandLow` packets.
    pub send_queue: BinaryHeap<SendRecord>,
    pub(crate) resend_task: Option<Task>,
}

/// A `Stream` and `Sink` of [`UdpPacket`]s, which always references the current
/// stream in the [`Data`] struct.
///
/// [`UdpPacket`]: ../packets/struct.UdpPacket.html
/// [`Data`]: struct.Data.html
pub struct DataUdpPackets<CS> {
    data: Weak<RefCell<Data<CS>>>,
}

/// A `Stream` and `Sink` of [`Packet`]s, which always references the current
/// stream in the [`Data`] struct.
///
/// [`Packet`]: ../packets/struct.Packet.html
/// [`Data`]: struct.Data.html
pub struct DataPackets<CS> {
    data: Weak<RefCell<Data<CS>>>,
}

/// Represents a currently alive connection.
pub struct Connection<ConnectionState> {
    /// The custom state of the connection.
    pub state: ConnectionState,
    /// The parameters of this connection, if it is already established.
    pub params: Option<ConnectedParams>,
    /// Smoothed round trip time.
    pub(crate) srtt: Duration,
    /// Deviation of the srtt.
    pub(crate) srtt_dev: Duration,
}

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

    /// The iv used to encrypt and decrypt packets.
    pub shared_iv: [u8; 20],
    /// The mac used for unencrypted packets.
    pub shared_mac: [u8; 8],
}

impl<State> Connection<State> {
    /// Creates a new connection state.
    pub fn new(state: State) -> Self {
        Self {
            state,
            params: None,
            srtt: Duration::milliseconds(2500),
            srtt_dev: Duration::milliseconds(0),
        }
    }

    /// Add another duration to the stored rtt.
    pub(crate) fn update_srtt(&mut self, rtt: Duration) {
        let diff = if rtt > self.srtt {
            rtt - self.srtt
        } else {
            self.srtt - rtt
        };
        self.srtt_dev = self.srtt_dev * 3 / 4 + diff / 4;
        self.srtt = self.srtt * 7 / 8 + rtt / 8;
    }
}

impl ConnectedParams {
    /// Fills the parameters for a connection with their default state.
    pub fn new(shared_iv: [u8; 20], shared_mac: [u8; 8]) -> Self {
        Self {
            outgoing_p_ids: Default::default(),
            receive_queue: Default::default(),
            fragmented_queue: Default::default(),
            incoming_p_ids: Default::default(),
            c_id: 0,
            voice_encryption: true,
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

impl<CS: 'static> Data<CS> {
    pub fn new<L: Into<Option<slog::Logger>>>(
        local_addr: SocketAddr,
        handle: Handle,
        is_client: bool,
        logger: L,
    ) -> Result<Rc<RefCell<Self>>> {
        let logger = logger.into().unwrap_or_else(|| {
            let decorator = slog_term::TermDecorator::new().build();
            //let decorator = slog_term::PlainSyncDecorator::new(::std::io::stdout());
            let drain = slog_term::FullFormat::new(decorator).build().fuse();
            let drain = slog_async::Async::new(drain).build().fuse();

            slog::Logger::root(drain, o!())
        });

        // TODO Don't create socket in this library
        let socket = UdpSocket::bind(&local_addr, &handle)?;
        let local_addr = socket.local_addr().unwrap_or(local_addr);
        let (sink, stream) = socket.framed(TsCodec::default()).split();
        let raw_sink = Box::new(sink.sink_map_err(|e| e.into()));
        let raw_stream = Box::new(stream.map_err(|e| e.into()));

        Ok(Rc::new(RefCell::new(Self {
            is_client,
            local_addr,
            handle,
            logger,
            udp_packet_stream: Some(raw_stream),
            udp_packet_sink: Some(raw_sink),
            raw_stream: None,
            raw_sink: None,
            packet_stream: None,
            packet_sink: None,
            connections: Default::default(),
            send_queue: Default::default(),
            resend_task: None,
        })))
    }

    pub fn add_outgoing_packet(
        data: Rc<RefCell<Self>>,
        p_type: PacketType,
        p_id: u16,
        packet: (SocketAddr, UdpPacket),
    ) {
        let now = Utc::now();
        let rec = SendRecord {
            sent: now,
            next: now,
            tries: 0,
            p_type,
            p_id,
            packet,
        };
        let mut data = data.borrow_mut();
        data.send_queue.push(rec);
        if let Some(ref task) = data.resend_task {
            task.notify();
        }
    }

    /// Gives a `Stream` and `Sink` of `UdpPacket`s, which always references the
    /// current stream in the `Data` struct.
    pub fn get_udp_packets(data: Rc<RefCell<Self>>) -> DataUdpPackets<CS> {
        DataUdpPackets { data: Rc::downgrade(&data) }
    }

    /// Gives a `Stream` and `Sink` of `Packet`s, which always references the
    /// current stream in the `Data` struct.
    pub fn get_packets(data: Rc<RefCell<Self>>) -> DataPackets<CS> {
        DataPackets { data: Rc::downgrade(&data) }
    }
}

impl<CS> Stream for DataUdpPackets<CS> {
    type Item = (SocketAddr, UdpPacket);
    type Error = Error;

    fn poll(&mut self) -> futures::Poll<Option<Self::Item>, Self::Error> {
        let data = self.data.upgrade().unwrap();
        let (mut stream, raw) = {
            let mut data = data.borrow_mut();
            data.udp_packet_stream
                .take()
                .map(|s| (s, false))
                .unwrap_or_else(|| (data.raw_stream.take().unwrap(), true))
        };
        let res = stream.poll();
        let mut data = data.borrow_mut();
        if raw {
            data.raw_stream = Some(stream);
        } else {
            data.udp_packet_stream = Some(stream);
        }
        res
    }
}

impl<CS> Sink for DataUdpPackets<CS> {
    type SinkItem = (SocketAddr, UdpPacket);
    type SinkError = Error;

    fn start_send(
        &mut self,
        item: Self::SinkItem,
    ) -> futures::StartSend<Self::SinkItem, Self::SinkError> {
        let data = self.data.upgrade().unwrap();
        let (mut sink, raw) = {
            let mut data = data.borrow_mut();
            data.udp_packet_sink
                .take()
                .map(|s| (s, false))
                .unwrap_or_else(|| (data.raw_sink.take().unwrap(), true))
        };
        let res = sink.start_send(item);
        let mut data = data.borrow_mut();
        if raw {
            data.raw_sink = Some(sink);
        } else {
            data.udp_packet_sink = Some(sink);
        }
        res
    }

    fn poll_complete(&mut self) -> futures::Poll<(), Self::SinkError> {
        let data = self.data.upgrade().unwrap();
        let (mut sink, raw) = {
            let mut data = data.borrow_mut();
            data.udp_packet_sink
                .take()
                .map(|s| (s, false))
                .unwrap_or_else(|| (data.raw_sink.take().unwrap(), true))
        };
        let res = sink.poll_complete();
        let mut data = data.borrow_mut();
        if raw {
            data.raw_sink = Some(sink);
        } else {
            data.udp_packet_sink = Some(sink);
        }
        res
    }

    fn close(&mut self) -> futures::Poll<(), Self::SinkError> {
        let data = self.data.upgrade().unwrap();
        let (mut sink, raw) = {
            let mut data = data.borrow_mut();
            data.udp_packet_sink
                .take()
                .map(|s| (s, false))
                .unwrap_or_else(|| (data.raw_sink.take().unwrap(), true))
        };
        let res = sink.close();
        let mut data = data.borrow_mut();
        if raw {
            data.raw_sink = Some(sink);
        } else {
            data.udp_packet_sink = Some(sink);
        }
        res
    }
}

impl<CS> Stream for DataPackets<CS> {
    type Item = (SocketAddr, Packet);
    type Error = Error;

    fn poll(&mut self) -> futures::Poll<Option<Self::Item>, Self::Error> {
        let data = self.data.upgrade().unwrap();
        let mut stream = data
            .borrow_mut()
            .packet_stream
            .take()
            .expect("Packet stream not available");
        let res = stream.poll();
        data.borrow_mut().packet_stream = Some(stream);
        res
    }
}

impl<CS> Sink for DataPackets<CS> {
    type SinkItem = (SocketAddr, Packet);
    type SinkError = Error;

    fn start_send(
        &mut self,
        item: Self::SinkItem,
    ) -> futures::StartSend<Self::SinkItem, Self::SinkError> {
        let data = self.data.upgrade().unwrap();
        let mut sink = data
            .borrow_mut()
            .packet_sink
            .take()
            .expect("Packet sink not available");
        let res = sink.start_send(item);
        data.borrow_mut().packet_sink = Some(sink);
        res
    }

    fn poll_complete(&mut self) -> futures::Poll<(), Self::SinkError> {
        let data = self.data.upgrade().unwrap();
        let mut sink = data
            .borrow_mut()
            .packet_sink
            .take()
            .expect("Packet sink not available");
        let res = sink.poll_complete();
        data.borrow_mut().packet_sink = Some(sink);
        res
    }

    fn close(&mut self) -> futures::Poll<(), Self::SinkError> {
        let data = self.data.upgrade().unwrap();
        let mut sink = data
            .borrow_mut()
            .packet_sink
            .take()
            .expect("Packet sink not available");
        let res = sink.close();
        data.borrow_mut().packet_sink = Some(sink);
        res
    }
}
