use std::cell::RefCell;
use std::net::SocketAddr;
use std::rc::{Rc, Weak};

use futures::{self, Sink, Stream};
use slog::Logger;

use Error;
use connectionmanager::ConnectionManager;
use handler_data::Data;
use packets::{Packet, UdpPacket};

pub struct PacketLogger;
impl PacketLogger {
    fn prepare_logger(
        logger: &Logger,
        addr: SocketAddr,
        is_client: bool,
        incoming: bool,
    ) -> Logger {
        let in_s = if incoming {
            "\x1b[1;32mIN\x1b[0m"
        } else {
            "\x1b[1;31mOUT\x1b[0m"
        };
        let to_s = if is_client { "S" } else { "C" };
        let addr_s = format!("{}", addr);
        logger.new(o!("dir" => in_s, "to" => to_s, "addr" => addr_s))
    }

    pub fn log_udp_packet(
        logger: &Logger,
        addr: SocketAddr,
        is_client: bool,
        incoming: bool,
        packet: &UdpPacket,
    ) {
        let logger = Self::prepare_logger(logger, addr, is_client, incoming);
        debug!(logger, "UdpPacket"; "content" => ?::HexSlice(&packet.0));
    }

    pub fn log_packet(
        logger: &Logger,
        addr: SocketAddr,
        is_client: bool,
        incoming: bool,
        packet: &Packet,
    ) {
        let logger = Self::prepare_logger(logger, addr, is_client, incoming);
        debug!(logger, "Packet"; "content" => ?packet);
    }
}

pub fn apply_udp_packet_logger<CM: ConnectionManager + 'static>(
    data: Rc<RefCell<Data<CM>>>) {
    let data2 = data.clone();
    let mut data = data.borrow_mut();
    let stream = UdpPacketStreamLogger::new(
        data2.clone(),
        data.udp_packet_stream.take().unwrap(),
        data.logger.clone(),
    );
    data.udp_packet_stream = Some(Box::new(stream));
    let sink = UdpPacketSinkLogger::new(
        data2,
        data.udp_packet_sink.take().unwrap(),
        data.logger.clone(),
    );
    data.udp_packet_sink = Some(Box::new(sink));
}

/* TODO
pub fn apply_packet_logger<CM: ConnectionManager + 'static>(
    data: Rc<RefCell<Data<CM>>>) {
    let data2 = data.clone();
    let mut data = data.borrow_mut();
    let stream = PacketStreamLogger::new(
        data2.clone(),
        data.packet_stream.take().unwrap(),
        data.logger.clone(),
    );
    data.packet_stream = Some(Box::new(stream));
    let sink = PacketSinkLogger::new(
        data2,
        data.packet_sink.take().unwrap(),
        data.logger.clone(),
    );
    data.packet_sink = Some(Box::new(sink));
}*/

pub struct UdpPacketStreamLogger<
    CM: ConnectionManager,
    Inner: Stream<Item = (SocketAddr, UdpPacket), Error = Error>,
> {
    data: Weak<RefCell<Data<CM>>>,
    inner: Inner,
    logger: Logger,
}

// TODO Add into_inner, get_ref, get_mut to all Streams and Sinks
impl<CM: ConnectionManager, Inner: Stream<Item = (SocketAddr, UdpPacket), Error = Error>>
    UdpPacketStreamLogger<CM, Inner> {
    pub fn new(
        data: Rc<RefCell<Data<CM>>>,
        inner: Inner,
        logger: Logger,
    ) -> Self {
        Self {
            data: Rc::downgrade(&data),
            inner,
            logger,
        }
    }
}

impl<CM: ConnectionManager, Inner: Stream<Item = (SocketAddr, UdpPacket), Error = Error>> Stream
    for UdpPacketStreamLogger<CM, Inner> {
    type Item = (SocketAddr, UdpPacket);
    type Error = Error;

    fn poll(&mut self) -> futures::Poll<Option<Self::Item>, Self::Error> {
        let res = self.inner.poll();
        if let Ok(futures::Async::Ready(Some((addr, ref packet)))) = res {
            let data = if let Some(data) = self.data.upgrade() {
                data
            } else {
                return Ok(futures::Async::Ready(None));
            };
            PacketLogger::log_udp_packet(
                &self.logger,
                addr,
                data.borrow().is_client,
                true,
                packet,
            );
        }
        res
    }
}

pub struct UdpPacketSinkLogger<
    CM: ConnectionManager,
    Inner: Sink<SinkItem = (SocketAddr, UdpPacket), SinkError = Error>,
> {
    data: Weak<RefCell<Data<CM>>>,
    inner: Inner,
    logger: Logger,
    /// The buffer to save a packet that is already logged.
    buf: Option<(SocketAddr, UdpPacket)>,
}

impl<
    CM: ConnectionManager,
    Inner: Sink<SinkItem = (SocketAddr, UdpPacket), SinkError = Error>,
> UdpPacketSinkLogger<CM, Inner> {
    pub fn new(
        data: Rc<RefCell<Data<CM>>>,
        inner: Inner,
        logger: Logger,
    ) -> Self {
        Self {
            data: Rc::downgrade(&data),
            inner,
            logger,
            buf: None,
        }
    }
}

impl<
    CM: ConnectionManager,
    Inner: Sink<SinkItem = (SocketAddr, UdpPacket), SinkError = Error>,
> Sink for UdpPacketSinkLogger<CM, Inner> {
    type SinkItem = (SocketAddr, UdpPacket);
    type SinkError = Error;

    fn start_send(
        &mut self,
        (addr, packet): Self::SinkItem,
    ) -> futures::StartSend<Self::SinkItem, Self::SinkError> {
        // Check if the buffer is full
        if let Some(p) = self.buf.take() {
            if let futures::AsyncSink::NotReady(p) = self.inner.start_send(p)? {
                self.buf = Some(p);
                return Ok(futures::AsyncSink::NotReady((addr, packet)));
            }
        }

        let data = self.data.upgrade().unwrap();
        PacketLogger::log_udp_packet(
            &self.logger,
            addr,
            data.borrow().is_client,
            false,
            &packet,
        );
        let res = self.inner.start_send((addr, packet))?;
        // Buffer the packet if it was not sent
        if let futures::AsyncSink::NotReady(p) = res {
            self.buf = Some(p);
            Ok(futures::AsyncSink::Ready)
        } else {
            Ok(res)
        }
    }

    fn poll_complete(&mut self) -> futures::Poll<(), Self::SinkError> {
        // Check if the buffer is full
        if let Some(p) = self.buf.take() {
            if let futures::AsyncSink::NotReady(p) = self.inner.start_send(p)? {
                self.buf = Some(p);
                return Ok(futures::Async::NotReady);
            }
        }

        self.inner.poll_complete()
    }

    fn close(&mut self) -> futures::Poll<(), Self::SinkError> {
        self.inner.poll_complete()
    }
}

pub struct PacketStreamLogger<
    CM: ConnectionManager,
    Inner: Stream<Item = (SocketAddr, Packet), Error = Error>,
> {
    data: Weak<RefCell<Data<CM>>>,
    inner: Inner,
    logger: Logger,
}

impl<CM: ConnectionManager, Inner: Stream<Item = (SocketAddr, Packet), Error = Error>>
    PacketStreamLogger<CM, Inner> {
    pub fn new(
        data: Rc<RefCell<Data<CM>>>,
        inner: Inner,
        logger: Logger,
    ) -> Self {
        Self {
            data: Rc::downgrade(&data),
            inner,
            logger,
        }
    }
}

impl<CM: ConnectionManager, Inner: Stream<Item = (SocketAddr, Packet), Error = Error>> Stream
    for PacketStreamLogger<CM, Inner> {
    type Item = (SocketAddr, Packet);
    type Error = Error;

    fn poll(&mut self) -> futures::Poll<Option<Self::Item>, Self::Error> {
        let res = self.inner.poll();
        if let Ok(futures::Async::Ready(Some((addr, ref packet)))) = res {
            let data = if let Some(data) = self.data.upgrade() {
                data
            } else {
                return Ok(futures::Async::Ready(None));
            };
            PacketLogger::log_packet(
                &self.logger,
                addr,
                data.borrow().is_client,
                true,
                packet,
            );
        }
        res
    }
}

pub struct PacketSinkLogger<
    CM: ConnectionManager,
    Inner: Sink<SinkItem = (SocketAddr, Packet), SinkError = Error>,
> {
    data: Weak<RefCell<Data<CM>>>,
    inner: Inner,
    logger: Logger,
}

impl<CM: ConnectionManager, Inner: Sink<SinkItem = (SocketAddr, Packet), SinkError = Error>>
    PacketSinkLogger<CM, Inner> {
    pub fn new(
        data: Rc<RefCell<Data<CM>>>,
        inner: Inner,
        logger: Logger,
    ) -> Self {
        Self {
            data: Rc::downgrade(&data),
            inner,
            logger,
        }
    }
}

impl<CM: ConnectionManager, Inner: Sink<SinkItem = (SocketAddr, Packet), SinkError = Error>> Sink
    for PacketSinkLogger<CM, Inner> {
    type SinkItem = (SocketAddr, Packet);
    type SinkError = Error;

    fn start_send(
        &mut self,
        (addr, packet): Self::SinkItem,
    ) -> futures::StartSend<Self::SinkItem, Self::SinkError> {
        let data = self.data.upgrade().unwrap();
        PacketLogger::log_packet(
            &self.logger,
            addr,
            data.borrow().is_client,
            false,
            &packet,
        );
        self.inner.start_send((addr, packet))
    }
    fn poll_complete(&mut self) -> futures::Poll<(), Self::SinkError> {
        self.inner.poll_complete()
    }
    fn close(&mut self) -> futures::Poll<(), Self::SinkError> {
        self.inner.poll_complete()
    }
}
