use std::{mem, ptr};
use std::net::SocketAddr;

use {evmap, futures_locks, slog, slog_async, slog_term, tokio};
use bytes::Bytes;
use futures::{self, Future, Sink, Stream};
use futures::sync::mpsc;
use slog::Drain;
use tokio::codec::BytesCodec;
use tokio::net::{UdpFramed, UdpSocket};

use {Error, Result};
use connection::*;
use connectionmanager::ConnectionManager;
use crypto::EccKeyPrivP256;
use packet_codec::PacketCodecReceiver;
use resend::DefaultResender;

pub type DataM<CM> = futures_locks::Mutex<Data<CM>>;

/// A listener for added and removed connections.
pub trait ConnectionListener<CM: ConnectionManager>: Send {
    /// Called when a new connection is created.
    ///
    /// Return `false` to remain in the list of listeners.
    /// If `true` is returned, this listener will be removed.
    ///
    /// Per default, `false` is returned.
    fn on_connection_created(&mut self, _data: &mut Data<CM>, _key: &mut CM::Key,
        _adata: &mut CM::AssociatedData, _con: &mut Connection) -> bool {
        false
    }

    /// Called when a connection is removed.
    ///
    /// Return `false` to remain in the list of listeners.
    /// If `true` is returned, this listener will be removed.
    ///
    /// Per default, `false` is returned.
    fn on_connection_removed(&mut self, _data: &mut Data<CM>, _key: &CM::Key,
        _con: &mut ConnectionValue<CM>) -> bool {
        false
    }
}

#[derive(Debug)]
pub struct ConnectionValue<CM: ConnectionManager + 'static> {
    pub mutex: futures_locks::Mutex<(CM::AssociatedData, Connection)>,
}

impl<CM: ConnectionManager + 'static> ConnectionValue<CM> {
    pub fn new(data: CM::AssociatedData, con: Connection) -> Self {
        Self { mutex: futures_locks::Mutex::new((data, con)) }
    }
}

impl<CM: ConnectionManager + 'static> PartialEq for ConnectionValue<CM> {
    fn eq(&self, other: &Self) -> bool {
        self as *const ConnectionValue<_> == other as *const _
    }
}

impl<CM: ConnectionManager + 'static> Eq for ConnectionValue<CM> {}

impl<CM: ConnectionManager + 'static> Clone for ConnectionValue<CM> {
    fn clone(&self) -> Self {
        Self { mutex: self.mutex.clone() }
    }
}

impl<CM: ConnectionManager + 'static> evmap::ShallowCopy for ConnectionValue<CM> {
    unsafe fn shallow_copy(&mut self) -> Self {
        // Try to copy without generating memory leaks
        let mut r: Self = mem::uninitialized();
        ptr::copy_nonoverlapping(self, &mut r, 1);
        r
    }
}

/// The stored data for our server or client.
///
/// This handles a single socket.
///
/// The list of connections is not managed by this struct, but it is passed to
/// an instance of [`ConnectionManager`] that is stored here.
///
/// [`ConnectionManager`]: trait.ConnectionManager.html
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
    /// [`add_connection`]: #method.add_connection
    /// [`remove_connection`]: #method.remove_connection
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
    ) -> Result<futures_locks::Mutex<Self>> {
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

        let data = Self {
            is_client,
            local_addr,
            private_key,
            logger,
            udp_packet_sink,
            resend_config: Default::default(),
            connections,
            connections_writer,
            connection_manager,
            connection_listeners: Vec::new(),
        };

        tokio::spawn(udp_packet_sink_sender.map(|(addr, p)|
            (p, addr))
            .forward(sink.sink_map_err(move |e|
                error!(logger2, "Failed to send udp packet"; "error" => ?e))
            ).map(|_| ()));

        // Handle incoming packets
        let logger = data.logger.clone();
        let codec = PacketCodecReceiver::new(&data);
        tokio::spawn(stream.from_err()
            .for_each(move |(p, a)| codec.handle_udp_packet((a, p)))
            .map_err(move |e| error!(logger, "Packet receiver failed";
                "error" => ?e)));
        Ok(futures_locks::Mutex::new(data))
    }

    pub fn create_connection(&self, addr: SocketAddr) -> Connection {
        // Add options like ip to logger
        let logger = self.logger.new(o!("addr" => addr.to_string()));
        let resender = DefaultResender::new(self.resend_config.clone(),
            logger.clone());
        Connection::new(addr, resender, logger, self.udp_packet_sink.clone(),
            self.is_client)
    }

    /// Add a new connection to this socket.
    ///
    /// The connection object can be created e. g. by the [`create_connection`]
    /// function.
    ///
    /// [`create_connection`]: #method.create_connection
    pub fn add_connection(&mut self, mut data: CM::AssociatedData,
        mut con: Connection) -> CM::Key {
        let mut key = self.connection_manager.new_connection_key(&mut data,
            &mut con);
        // Call listeners
        let mut i = 0;
        while i < self.connection_listeners.len() {
            if self.connection_listeners[i].on_connection_created(
                self, &mut key, &mut data, &mut con) {
                self.connection_listeners.remove(i);
            } else {
                i += 1;
            }
        }

        // Add connection
        self.connections_writer.insert(key.clone(), ConnectionValue::new(data, con));
        self.connections_writer.refresh();
        key
    }

    pub fn remove_connection(&mut self, key: &CM::Key)
        -> Option<ConnectionValue<CM>> {
        // Get connection
        let mut res = self.get_connection(key);
        if let Some(con) = &mut res {
            // Remove connection
            self.connections_writer.clear(key.clone());
            self.connections_writer.refresh();

            // Call listeners
            let mut i = 0;
            while i < self.connection_listeners.len() {
                if self.connection_listeners[i].on_connection_removed(
                    self, &key, con) {
                    self.connection_listeners.remove(i);
                } else {
                    i += 1;
                }
            }
        }
        res
    }

    pub fn get_connection(&self, key: &CM::Key) -> Option<ConnectionValue<CM>> {
        self.connections.get_and(&key, |v| v[0].clone())
    }
}
