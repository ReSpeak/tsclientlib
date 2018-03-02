use std::cell::RefCell;
use std::collections::VecDeque;
use std::mem;
use std::net::SocketAddr;
use std::rc::{Rc, Weak};

use {slog, slog_async, slog_term};
use futures::{self, Future, Sink, Stream, task};
use futures::task::Task;
use slog::Drain;
use tokio_core::net::UdpSocket;
use tokio_core::reactor::Handle;

use {Error, Result, TsCodec, StreamWrapper, SinkWrapper};
use connection::*;
use connectionmanager::ConnectionManager;
use packets::*;

/// A listener for added and removed connections.
pub trait ConnectionListener<CM: ConnectionManager> {
    /// Called when a new connection is created.
    ///
    /// Return `false` to remain in the list of listeners.
    /// If `true` is returned, this listener will be removed.
    fn on_connection_created(&mut self, _data: Rc<RefCell<Data<CM>>>,
        _key: CM::ConnectionsKey) -> bool {
        false
    }

    /// Called when a connection is removed.
    ///
    /// Return `false` to remain in the list of listeners.
    /// If `true` is returned, this listener will be removed.
    fn on_connection_removed(&mut self, _data: Rc<RefCell<Data<CM>>>,
        _key: CM::ConnectionsKey) -> bool {
        false
    }
}

/// The stored data for our server or client.
///
/// This data is stored for one socket.
///
/// The list of connections is not managed by this struct, but it is passed to
/// an instance of [`ConnectionManager`] that is stored here.
///
/// [`ConnectionManager`]:
pub struct Data<CM: ConnectionManager> {
    /// If this structure is owned by a client or a server.
    pub is_client: bool,
    /// The address of the socket.
    pub local_addr: SocketAddr,
    /// The private key of this instance.
    pub private_key: ::crypto::EccKey,
    pub handle: Handle,
    pub logger: slog::Logger,

    /// The stream of `UdpPacket`s.
    ///
    /// [`UdpPacket`]: ../packets/struct.UdpPacket.html
    pub udp_packet_stream:
        Option<Box<Stream<Item = (SocketAddr, UdpPacket), Error = Error>>>,
    /// The sink of [`UdpPacket`]s.
    ///
    /// [`UdpPacket`]: ../packets/struct.UdpPacket.html
    pub udp_packet_sink:
        Option<Box<Sink<SinkItem = (SocketAddr, UdpPacket),
            SinkError = Error>>>,

    /// The stream of `UdpPacket`s which don't belong to any connection.
    ///
    /// [`UdpPacket`]: ../packets/struct.UdpPacket.html
    pub unknown_udp_packet_stream:
        Option<Box<Stream<Item = (SocketAddr, UdpPacket), Error = Error>>>,
    unknown_stream_buffer: VecDeque<(SocketAddr, UdpPacket)>,
    unknown_stream_task: Option<Task>,

    /// The task of the packet distributor.
    distributor_task: Option<Task>,

    /// A list of all connected clients or servers
    ///
    /// You should not add or remove connections directly using the manager,
    /// unless you know what you are doing (e.g. connection listeners are not
    /// called).
    /// Instead, use the [`add_connection`] und [`remove_connection`] function.
    ///
    /// [`add_connection`]:
    /// [`remove_connection`]:
    pub connection_manager: CM,
    /// Listen for new or removed connections.
    pub connection_listeners: Vec<Box<ConnectionListener<CM>>>,
}

impl<CM: ConnectionManager + 'static> Data<CM> {
    /// An optional logger can be provided. If none is provided, a new one will
    /// be created.
    pub fn new<L: Into<Option<slog::Logger>>>(
        local_addr: SocketAddr,
        private_key: ::crypto::EccKey,
        handle: Handle,
        is_client: bool,
        connection_manager: CM,
        logger: L,
    ) -> Result<Rc<RefCell<Self>>> {
        let logger = logger.into().unwrap_or_else(|| {
            let decorator = slog_term::TermDecorator::new().build();
            let drain = slog_term::FullFormat::new(decorator).build().fuse();
            let drain = slog_async::Async::new(drain).build().fuse();

            slog::Logger::root(drain, o!())
        });

        // Create the socket
        let socket = UdpSocket::bind(&local_addr, &handle)?;
        let local_addr = socket.local_addr().unwrap_or(local_addr);
        let (sink, stream) = socket.framed(TsCodec::default()).split();
        let sink = Box::new(sink.sink_map_err(|e| e.into()));
        let stream = Box::new(stream.map_err(|e| e.into()));

        let data = Rc::new(RefCell::new(Self {
            is_client,
            local_addr,
            private_key,
            handle,
            logger,
            udp_packet_stream: Some(stream),
            udp_packet_sink: Some(sink),
            unknown_udp_packet_stream: None,
            unknown_stream_buffer: Default::default(),
            unknown_stream_task: None,
            distributor_task: None,
            connection_manager,
            connection_listeners: Vec::new(),
        }));

        // Set stream for unknown packets
        let stream = UnknownUdpPacketStream::new(Rc::downgrade(&data));
        data.borrow_mut().unknown_udp_packet_stream = Some(Box::new(stream));

        Ok(data)
    }

    pub fn create_connection(data: &Rc<RefCell<Self>>, addr: SocketAddr)
        -> Rc<RefCell<Connection<CM>>> {
        let resender = {
            let data = data.borrow();
            data.connection_manager.create_resender()
        };

        Connection::new(data, addr, resender)
    }

    /// Add a new connection to this socket.
    ///
    /// The connection object can be created e. g. by the [`create_connection`]
    /// function.
    ///
    /// [`create_connection`]:
    pub fn add_connection(data: &Rc<RefCell<Self>>,
        connection: Rc<RefCell<Connection<CM>>>) -> CM::ConnectionsKey {
        let mut tmp;
        // Add connection and take listeners
        let key = {
            let data = &mut *data.borrow_mut();
            let key = data.connection_manager.add_connection(connection,
                &data.handle);
            tmp = mem::replace(&mut data.connection_listeners, Vec::new());
            key
        };
        let key2 = key.clone();
        let mut tmp = tmp.drain(..)
            .filter_map(|mut l| if l.on_connection_created(data.clone(),
                key2.clone()) {
                None
            } else {
                Some(l)
            })
            .collect::<Vec<_>>();
        // Put listeners back
        {
            let mut data = data.borrow_mut();
            tmp.append(&mut data.connection_listeners);
            data.connection_listeners = tmp;
        }
        key
    }

    pub fn remove_connection(data: &Rc<RefCell<Self>>, key: CM::ConnectionsKey) {
        let mut tmp;
        // Take listeners
        {
            let mut data = data.borrow_mut();
            tmp = mem::replace(&mut data.connection_listeners, Vec::new());
        }
        let key2 = key.clone();
        let mut tmp = tmp.drain(..)
            .filter_map(|mut l| if l.on_connection_removed(data.clone(),
                key2.clone()) {
                None
            } else {
                Some(l)
            })
            .collect::<Vec<_>>();
        // Put listeners back and remove connection
        {
            let mut data = data.borrow_mut();
            tmp.append(&mut data.connection_listeners);
            data.connection_listeners = tmp;
            data.connection_manager.remove_connection(key);
        }
    }

    pub fn apply_udp_packet_stream_wrapper<
        W: StreamWrapper<(SocketAddr, UdpPacket), Error,
            Box<Stream<Item = (SocketAddr, UdpPacket), Error = Error>>>
            + 'static,
    >(data: &Rc<RefCell<Self>>, a: W::A) {
        let mut data = data.borrow_mut();
        let inner = data.udp_packet_stream.take().unwrap();
        data.udp_packet_stream = Some(Box::new(W::wrap(inner, a)));
    }

    pub fn apply_udp_packet_sink_wrapper<
        W: SinkWrapper<(SocketAddr, UdpPacket), Error,
            Box<Sink<SinkItem = (SocketAddr, UdpPacket), SinkError = Error>>>
            + 'static,
    >(data: &Rc<RefCell<Self>>, a: W::A) {
        let mut data = data.borrow_mut();
        let inner = data.udp_packet_sink.take().unwrap();
        data.udp_packet_sink = Some(Box::new(W::wrap(inner, a)));
    }

    pub fn apply_unknown_udp_packet_stream_wrapper<
        W: StreamWrapper<(SocketAddr, UdpPacket), Error,
            Box<Stream<Item = (SocketAddr, UdpPacket), Error = Error>>>
            + 'static,
    >(data: &Rc<RefCell<Self>>, a: W::A) {
        let mut data = data.borrow_mut();
        let inner = data.unknown_udp_packet_stream.take().unwrap();
        data.unknown_udp_packet_stream = Some(Box::new(W::wrap(inner, a)));
    }

    /// Gives a `Stream` and `Sink` of `UdpPacket`s, which always references the
    /// current stream in the `Data` struct.
    pub fn get_udp_packets(data: Weak<RefCell<Self>>) -> DataUdpPackets<CM> {
        DataUdpPackets { data }
    }

    /// Gives a `Stream` of `UdpPacket`s, which always references the current
    /// unknown stream in the `Data` struct.
    ///
    /// This stream contains all the packets for which no connection is known.
    pub fn get_unknown_udp_packets(data: Weak<RefCell<Self>>) ->
        UnknownDataUdpPackets<CM> {
        UnknownDataUdpPackets { data }
    }

    /// Enables distributing incoming packets to the connections.
    pub fn start_packet_distributor(data: &Rc<RefCell<Self>>) {
        let distributor = PacketDistributor::new(
            Self::get_udp_packets(Rc::downgrade(data)), Rc::downgrade(data));
        let data = data.borrow_mut();
        let logger = data.logger.clone();
        data.handle.spawn(distributor.map_err(move |e| {
            error!(logger, "Distributor exited with error"; "error" => ?e);
        }));
    }
}

impl<CM: ConnectionManager> Drop for Data<CM> {
    fn drop(&mut self) {
        // Drop the distributor if we are dropped
        if let Some(ref task) = self.distributor_task {
            task.notify();
        }
        if let Some(ref task) = self.unknown_stream_task {
            task.notify();
        }
    }
}

/// A `Stream` and `Sink` of [`UdpPacket`]s, which always references the current
/// stream in the [`Data`] struct.
///
/// [`UdpPacket`]: ../packets/struct.UdpPacket.html
/// [`Data`]: struct.Data.html
pub struct DataUdpPackets<CM: ConnectionManager> {
    data: Weak<RefCell<Data<CM>>>,
}

impl<CM: ConnectionManager> Stream for DataUdpPackets<CM> {
    type Item = (SocketAddr, UdpPacket);
    type Error = Error;

    fn poll(&mut self) -> futures::Poll<Option<Self::Item>, Self::Error> {
        let data = if let Some(data) = self.data.upgrade() {
            data
        } else {
            return Ok(futures::Async::Ready(None));
        };
        let mut stream = {
            let mut data = data.borrow_mut();
            data.udp_packet_stream
                .take()
                .unwrap()
        };
        let res = stream.poll();
        let mut data = data.borrow_mut();
        data.udp_packet_stream = Some(stream);
        res
    }
}

impl<CM: ConnectionManager> Sink for DataUdpPackets<CM> {
    type SinkItem = (SocketAddr, UdpPacket);
    type SinkError = Error;

    fn start_send(
        &mut self,
        item: Self::SinkItem,
    ) -> futures::StartSend<Self::SinkItem, Self::SinkError> {
        let data = self.data.upgrade().unwrap();
        let mut sink = {
            let mut data = data.borrow_mut();
            data.udp_packet_sink
                .take()
                .unwrap()
        };
        let res = sink.start_send(item);
        let mut data = data.borrow_mut();
        data.udp_packet_sink = Some(sink);
        res
    }

    fn poll_complete(&mut self) -> futures::Poll<(), Self::SinkError> {
        let data = self.data.upgrade().unwrap();
        let mut sink = {
            let mut data = data.borrow_mut();
            data.udp_packet_sink
                .take()
                .unwrap()
        };
        let res = sink.poll_complete();
        let mut data = data.borrow_mut();
        data.udp_packet_sink = Some(sink);
        res
    }

    fn close(&mut self) -> futures::Poll<(), Self::SinkError> {
        let data = self.data.upgrade().unwrap();
        let mut sink = {
            let mut data = data.borrow_mut();
            data.udp_packet_sink
                .take()
                .unwrap()
        };
        let res = sink.close();
        let mut data = data.borrow_mut();
        data.udp_packet_sink = Some(sink);
        res
    }
}

/// A `Stream` of [`UdpPacket`]s, which always references the current unknown
/// stream in the [`Data`] struct.
///
/// This stream contains all packets for which no connection is known.
///
/// [`UdpPacket`]: ../packets/struct.UdpPacket.html
/// [`Data`]: struct.Data.html
pub struct UnknownDataUdpPackets<CM: ConnectionManager> {
    data: Weak<RefCell<Data<CM>>>,
}

impl<CM: ConnectionManager> Stream for UnknownDataUdpPackets<CM> {
    type Item = (SocketAddr, UdpPacket);
    type Error = Error;

    fn poll(&mut self) -> futures::Poll<Option<Self::Item>, Self::Error> {
        let data = if let Some(data) = self.data.upgrade() {
            data
        } else {
            return Ok(futures::Async::Ready(None));
        };
        let mut stream = {
            let mut data = data.borrow_mut();
            data.unknown_udp_packet_stream
                .take()
                .unwrap()
        };
        let res = stream.poll();
        let mut data = data.borrow_mut();
        data.unknown_udp_packet_stream = Some(stream);
        res
    }
}

/// The stream which fetches packets from the `Data` object.
struct UnknownUdpPacketStream<CM: ConnectionManager> {
    data: Weak<RefCell<Data<CM>>>,
}

impl<CM: ConnectionManager> UnknownUdpPacketStream<CM> {
    fn new(data: Weak<RefCell<Data<CM>>>) -> Self {
        Self { data }
    }
}

impl<CM: ConnectionManager> Stream for UnknownUdpPacketStream<CM> {
    type Item = (SocketAddr, UdpPacket);
    type Error = Error;

    fn poll(&mut self) -> futures::Poll<Option<Self::Item>, Self::Error> {
        let data = if let Some(data) = self.data.upgrade() {
            data
        } else {
            // The connection does not exist anymore, just quit
            return Ok(futures::Async::Ready(None));
        };
        // Set task
        let mut data = data.borrow_mut();
        data.unknown_stream_task = Some(task::current());

        // Check if there is a packet available
        if let Some(packet) = data.unknown_stream_buffer.pop_front() {
            Ok(futures::Async::Ready(Some(packet)))
        } else {
            Ok(futures::Async::NotReady)
        }
    }
}

pub struct PacketDistributor<
    Inner: Stream<Item = (SocketAddr, UdpPacket), Error = Error>,
    CM: ConnectionManager,
> {
    inner: Inner,
    data: Weak<RefCell<Data<CM>>>,
}

impl<
    Inner: Stream<Item = (SocketAddr, UdpPacket), Error = Error>,
    CM: ConnectionManager,
> PacketDistributor<Inner, CM> {
    fn new(inner: Inner, data: Weak<RefCell<Data<CM>>>) -> Self {
        Self { inner, data }
    }
}

impl<
    Inner: Stream<Item = (SocketAddr, UdpPacket), Error = Error>,
    CM: ConnectionManager + 'static,
> Future for PacketDistributor<Inner, CM> {
    type Item = ();
    type Error = Error;

    fn poll(&mut self) -> futures::Poll<Self::Item, Self::Error> {
        let data = if let Some(data) = self.data.upgrade() {
            data
        } else {
            return Ok(futures::Async::Ready(()));
        };
        // Set the task
        {
            let mut data = data.borrow_mut();
            data.distributor_task = Some(task::current());
        }

        // Check if a packet is available
        if let futures::Async::Ready(res) = self.inner.poll()? {
            if let Some((addr, packet)) = res {
                // Find the connection
                let mut data = data.borrow_mut();
                let con = data.connection_manager.get_connection_for_udp_packet(
                    addr, &packet);
                if let Some(con) = con {
                    let mut con = con.borrow_mut();
                    if con.udp_packet_buffer_stream.buffer.len() >=
                        ::STREAM_BUFFER_MAX_SIZE {
                        warn!(data.logger,
                            "Dropping packet, stream buffer too full";
                            "length" => con.udp_packet_buffer_stream.buffer
                                .len());
                    } else {
                        // Add packet to queue and notify stream
                        con.udp_packet_buffer_stream.buffer.push_back(packet);

                        if let Some(ref task) = con.udp_packet_buffer_stream.task {
                            task.notify();
                        } else {
                            warn!(data.logger,
                                "Distributor found no stream task");
                        }
                    }
                } else if data.unknown_stream_buffer.len()
                    >= ::STREAM_BUFFER_MAX_SIZE {
                    warn!(data.logger,
                        "Dropping packet, unknown stream buffer too full");
                } else {
                    // Add packet to queue and notify stream
                    data.unknown_stream_buffer.push_back((addr, packet));

                    if let Some(ref task) = data.unknown_stream_task {
                        task.notify();
                    } else {
                        warn!(data.logger,
                            "Distributor found no unknown stream task");
                    }
                }

                // Request next packet
                task::current().notify();
            } else {
                // Stream ended
                let data = data.borrow();
                warn!(data.logger, "Stream for distributor ended");
                return Ok(futures::Async::Ready(()));
            }
        }

        Ok(futures::Async::NotReady)
    }
}
