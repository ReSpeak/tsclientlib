use std::cell::RefCell;
use std::mem;
use std::net::SocketAddr;
use std::rc::{Rc, Weak};

use {evmap, futures_locks, slog, slog_async, slog_term, tokio};
use bytes::Bytes;
use futures::{self, Future, Sink, Stream};
use futures::sync::mpsc;
use slog::Drain;
use tokio::codec::BytesCodec;
use tokio::net::{UdpFramed, UdpSocket};

use {Error, Result, StreamWrapper, SinkWrapper};
use connection::*;
use connectionmanager::ConnectionManager;
use crypto::EccKeyPrivP256;
use packets::*;

/// A listener for added and removed connections.
pub trait ConnectionListener<CM: ConnectionManager> {
    /// Called when a new connection is created.
    ///
    /// Return `false` to remain in the list of listeners.
    /// If `true` is returned, this listener will be removed.
    fn on_connection_created(&mut self, _data: Rc<RefCell<Data<CM>>>,
        _key: CM::Key) -> bool {
        false
    }

    /// Called when a connection is removed.
    ///
    /// Return `false` to remain in the list of listeners.
    /// If `true` is returned, this listener will be removed.
    fn on_connection_removed(&mut self, _data: Rc<RefCell<Data<CM>>>,
        _key: CM::Key) -> bool {
        false
    }
}

pub struct ConnectionValue<CM: ConnectionManager + 'static> {
    pub mutex: futures_locks::Mutex<(CM::AssociatedData, Connection)>,
}

impl<CM: ConnectionManager + 'static> PartialEq for ConnectionValue<CM> {
    fn eq(&self, other: &Self) -> bool {
        self as *const ConnectionValue<_> == other as *const _
    }
}

impl<CM: ConnectionManager + 'static> Eq for ConnectionValue<CM> {}

impl<CM: ConnectionManager + 'static> evmap::ShallowCopy for ConnectionValue<CM> {
    unsafe fn shallow_copy(&mut self) -> Self {
        // Try to copy without generating memory leaks
        let r: Self = mem::uninitialized();
        panic!()
    }
}

/// The stored data for our server or client.
///
/// This handles a single socket.
///
/// The list of connections is not managed by this struct, but it is passed to
/// an instance of [`ConnectionManager`] that is stored here.
///
/// [`ConnectionManager`]:
pub struct Data<CM: ConnectionManager + 'static> {
    /// If this structure is owned by a client or a server.
    pub is_client: bool,
    /// The address of the socket.
    pub local_addr: SocketAddr,
    /// The private key of this instance.
    pub private_key: EccKeyPrivP256,
    pub logger: slog::Logger,

    /// The sink of udp packets.
    pub udp_packet_sink: mpsc::Sender<(SocketAddr, Bytes)>,

    /// The stream of `Packet`s.
    pub packet_stream:
        Option<Box<Stream<Item = (CM::Key, Packet), Error = Error>>>,
    /// The sink of `Packet`s.
    pub packet_sink:
        Option<Box<Sink<SinkItem = (CM::Key, Packet), SinkError = Error>>>,

    /// The default resend config. It gets copied for each new connection.
    resend_config: ::resend::ResendConfig,

    pub connections: evmap::ReadHandle<CM::Key, ConnectionValue<CM>>,
    pub connections_writer: evmap::WriteHandle<CM::Key, ConnectionValue<CM>>,

    /// A list of all connected clients or servers
    ///
    /// You should not add or remove connections directly using the manager,
    /// unless you know what you are doing. E.g. connection listeners are not
    /// called if you do so.
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
        private_key: EccKeyPrivP256,
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
        let socket = UdpSocket::bind(&local_addr)?;
        let local_addr = socket.local_addr().unwrap_or(local_addr);
        debug!(logger, "Listening"; "local_addr" => %local_addr);
        let (sink, stream) = UdpFramed::new(socket, BytesCodec::new()).split();

        let (udp_packet_sink, udp_packet_sink_sender) = mpsc::channel(::UDP_SINK_CAPACITY);
        let logger2 = logger.clone();

        let (connections, connections_writer) = evmap::new();

        let data = Rc::new(RefCell::new(Self {
            is_client,
            local_addr,
            private_key,
            logger,
            udp_packet_sink,
            packet_stream: None,
            packet_sink: None,
            resend_config: Default::default(),
            connections,
            connections_writer,
            connection_manager,
            connection_listeners: Vec::new(),
        }));

        tokio::spawn(udp_packet_sink_sender.map(|(addr, p)|
            (p, addr))
            .forward(sink.sink_map_err(move |e|
                error!(logger2, "Failed to send udp packet"; "error" => ?e))
            ).map(|_| ()));

        // TODO Use PacketCodecReceiver::handle_udp_packet with stream
        Ok(data)
    }

    pub fn create_connection(data: &Rc<RefCell<Self>>, addr: SocketAddr)
        -> Rc<RefCell<Connection>> {
        // Add options like ip to logger
        let (resender, logger, sink, is_client) = {
            let data = data.borrow();
            let logger = data.logger.new(o!("addr" => addr.to_string()));

            (::resend::DefaultResender::new(data.resend_config.clone(),
                logger.clone()), logger, data.udp_packet_sink.clone(),
                data.is_client)
        };

        Connection::new(addr, resender, logger, sink, is_client)
    }

    /// Add a new connection to this socket.
    ///
    /// The connection object can be created e. g. by the [`create_connection`]
    /// function.
    ///
    /// [`create_connection`]:
    pub fn add_connection(data: &Rc<RefCell<Self>>,
        connection: Rc<RefCell<Connection>>) -> CM::Key {
        let mut tmp;
        // Add connection and take listeners
        let key = {
            let data = &mut *data.borrow_mut();
            let key = data.connection_manager.add_connection(connection);
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

    pub fn remove_connection(data: &Rc<RefCell<Self>>, key: CM::Key) {
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

    pub fn apply_packet_stream_wrapper<
        W: StreamWrapper<(CM::Key, Packet), Error,
            Box<Stream<Item = (CM::Key, Packet), Error = Error>>>
            + 'static,
    >(data: &Rc<RefCell<Self>>, a: W::A) {
        let mut data = data.borrow_mut();
        let inner = data.packet_stream.take().unwrap();
        data.packet_stream = Some(Box::new(W::wrap(inner, a)));
    }

    pub fn apply_packet_sink_wrapper<
        W: SinkWrapper<(CM::Key, Packet), Error,
            Box<Sink<SinkItem = (CM::Key, Packet), SinkError = Error>>>
            + 'static,
    >(data: &Rc<RefCell<Self>>, a: W::A) {
        let mut data = data.borrow_mut();
        let inner = data.packet_sink.take().unwrap();
        data.packet_sink = Some(Box::new(W::wrap(inner, a)));
    }

    /// Gives a `Stream` and `Sink` of `Packet`s, which always references the
    /// current stream in the `Data` struct.
    pub fn get_packets(data: Weak<RefCell<Self>>) -> DataPackets<CM> {
        DataPackets { data }
    }
}

/// A `Stream` and `Sink` of [`Packet`]s, which always references the current
/// stream in the [`Data`] struct.
///
/// [`Packet`]: ../packets/struct.Packet.html
/// [`Data`]: struct.Data.html
pub struct DataPackets<CM: ConnectionManager + 'static> {
    data: Weak<RefCell<Data<CM>>>,
}

impl<CM: ConnectionManager + 'static> Stream for DataPackets<CM> {
    type Item = (CM::Key, Packet);
    type Error = Error;

    fn poll(&mut self) -> futures::Poll<Option<Self::Item>, Self::Error> {
        let data = if let Some(data) = self.data.upgrade() {
            data
        } else {
            return Ok(futures::Async::Ready(None));
        };
        let mut stream = {
            let mut data = data.borrow_mut();
            data.packet_stream
                .take()
                .unwrap()
        };
        let res = stream.poll();
        let mut data = data.borrow_mut();
        data.packet_stream = Some(stream);
        res
    }
}

impl<CM: ConnectionManager + 'static> Sink for DataPackets<CM> {
    type SinkItem = (CM::Key, Packet);
    type SinkError = Error;

    fn start_send(
        &mut self,
        item: Self::SinkItem,
    ) -> futures::StartSend<Self::SinkItem, Self::SinkError> {
        let data = self.data.upgrade().unwrap();
        let mut sink = {
            let mut data = data.borrow_mut();
            data.packet_sink
                .take()
                .unwrap()
        };
        let res = sink.start_send(item);
        let mut data = data.borrow_mut();
        data.packet_sink = Some(sink);
        res
    }

    fn poll_complete(&mut self) -> futures::Poll<(), Self::SinkError> {
        let data = self.data.upgrade().unwrap();
        let mut sink = {
            let mut data = data.borrow_mut();
            data.packet_sink
                .take()
                .unwrap()
        };
        let res = sink.poll_complete();
        let mut data = data.borrow_mut();
        data.packet_sink = Some(sink);
        res
    }

    fn close(&mut self) -> futures::Poll<(), Self::SinkError> {
        let data = self.data.upgrade().unwrap();
        let mut sink = {
            let mut data = data.borrow_mut();
            data.packet_sink
                .take()
                .unwrap()
        };
        let res = sink.close();
        let mut data = data.borrow_mut();
        data.packet_sink = Some(sink);
        res
    }
}
