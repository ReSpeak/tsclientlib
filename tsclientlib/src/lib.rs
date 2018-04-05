//! tsclientlib is a library which makes it simple to create TeamSpeak clients
//! and bots.
//!
//! If you want a full client application, you might want to have a look at
//! [Qint].
//!
//! The base class of this library is the [`ConnectionManager`]. From there you
//! can create, inspect, modify and remove connections.
//!
//! [`ConnectionManager`]: struct.ConnectionManager.html
//! [Qint]: https://github.com/ReSpeak/Qint

// # Internal structure
// ConnectionManager is a wrapper around Rc<RefCell<InnerCM>>,
// it contains the api to create and destroy connections.
// To inspect/modify things, facade objects are used (included in lib.rs).
// All facade objects exist in a mutable and non-mutable version and they borrow
// the ConnectionManager.
// That means all references have to be handed back before the ConnectionManager
// can be used again.
//
// InnerCM contains a Map<ConnectionId, structs::NetworkWrapper>.
//
// NetworkWrapper contains the Connection which is the bookkeeping struct,
// the Rc<RefCell<client::ClientData>>, which is the tsproto connection,
// a reference to the ClientConnection of tsproto
// and a stream of Messages.
//
// The NetworkWrapper wraps the stream of Messages and updates the
// bookkeeping, raises events, etc. on new packets.
// The ConnectionManager wraps all those streams into one stream (like a
// select). To progress, the user of the library has to poll the
// ConnectionManager for new notifications and sound.
//
// The items of the stream are either Messages or audio data.

// TODO
#![allow(dead_code)]

extern crate base64;
extern crate chrono;
#[macro_use]
extern crate failure;
extern crate futures;
#[macro_use]
extern crate slog;
extern crate slog_async;
extern crate slog_perf;
extern crate slog_term;
extern crate tokio_core;
extern crate tsproto;
extern crate tsproto_commands;

use std::cell::{Ref, RefCell, RefMut};
use std::fmt;
use std::mem;
use std::net::SocketAddr;
use std::sync::{Once, ONCE_INIT};
use std::rc::Rc;

use chrono::{DateTime, Duration, Utc};
use failure::ResultExt;
use futures::{future, Future, Sink, Stream};
use futures::task::{self, Task};
use futures::future::Either;
use slog::{Drain, Logger};
use tokio_core::reactor::Handle;
use tsproto::algorithms as algs;
use tsproto::{client, crypto, packets, commands};
use tsproto::connectionmanager::ConnectionManager as TsprotoCM;
use tsproto::connectionmanager::{Resender, ResenderEvent};
use tsproto::handler_data::Data;
use tsproto::packets::{Header, Packet, PacketType};
use tsproto_commands::*;

macro_rules! copy_attrs {
    ($from:ident, $to:ident; $($attr:ident),* $(,)*; $($extra:ident: $ex:expr),* $(,)*) => {
        $to {
            $($attr: $from.$attr.clone(),)*
            $($extra: $ex,)*
        }
    };
}

macro_rules! tryf {
    ($e:expr) => {
        match $e {
            Ok(e) => e,
            Err(error) => return Box::new(future::err(error.into())),
        }
    };
}

pub mod codec;
mod structs;

// Reexports
pub use tsproto_commands::ConnectionId;
pub use tsproto_commands::Reason;
pub use tsproto_commands::versions::Version;
use tsproto_commands::messages;


use codec::Message;

type Result<T> = std::result::Result<T, Error>;
type BoxFuture<T> = Box<Future<Item = T, Error = Error>>;
type Map<K, V> = std::collections::HashMap<K, V>;

include!(concat!(env!("OUT_DIR"), "/facades.rs"));
include!(concat!(env!("OUT_DIR"), "/getters.rs"));

#[derive(Fail, Debug)]
pub enum Error {
    #[fail(display = "Connection failed ({})", _0)]
    ConnectionFailed(String),
    #[fail(display = "{}", _0)]
    Base64(#[cause] base64::DecodeError),
    #[fail(display = "{}", _0)]
    Tsproto(#[cause] tsproto::Error),
    #[fail(display = "{}", _0)]
    ParseMessage(#[cause] tsproto_commands::messages::ParseError),
    #[fail(display = "{}", _0)]
    Other(#[cause] failure::Compat<failure::Error>),
}

impl From<base64::DecodeError> for Error {
    fn from(e: base64::DecodeError) -> Self {
        Error::Base64(e)
    }
}

impl From<tsproto::Error> for Error {
    fn from(e: tsproto::Error) -> Self {
        Error::Tsproto(e)
    }
}

impl From<tsproto_commands::messages::ParseError> for Error {
    fn from(e: tsproto_commands::messages::ParseError) -> Self {
        Error::ParseMessage(e)
    }
}

impl From<failure::Error> for Error {
    fn from(e: failure::Error) -> Self {
        let r: std::result::Result<(), _> = Err(e);
        Error::Other(r.compat().unwrap_err())
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub enum ChannelType {
    Permanent,
    SemiPermanent,
    Temporary,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub enum MaxFamilyClients {
    Unlimited,
    Inherited,
    Limited(u16),
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct TalkPowerRequest {
    pub time: DateTime<Utc>,
    pub message: String,
}

/// The connection manager which can be shared and cloned.
struct InnerCM {
    handle: Handle,
    logger: Logger,
    connections: Map<ConnectionId, structs::NetworkWrapper>,
}

impl InnerCM {
    /// Returns the first free connection id.
    fn find_connection_id(&self) -> ConnectionId {
        for i in 0..self.connections.len() + 1 {
            let id = ConnectionId(i);
            if !self.connections.contains_key(&id) {
                return id;
            }
        }
        unreachable!("Found no free connection id, this should not happen");
    }
}

/// The main type of this crate, which holds all connections.
///
/// It can be created with the [`ConnectionManager::new`] function:
///
/// ```
/// # extern crate tokio_core;
/// # extern crate tsclientlib;
/// # use std::boxed::Box;
/// # use std::error::Error;
/// #
/// # use tsclientlib::ConnectionManager;
/// # fn main() {
/// #
/// let core = tokio_core::reactor::Core::new().unwrap();
/// let cm = ConnectionManager::new(core.handle());
/// # }
/// ```
///
/// [`ConnectionManager::new`]: #method.new
pub struct ConnectionManager {
    inner: Rc<RefCell<InnerCM>>,
    /// The index of the connection which should be polled next.
    ///
    /// This is used to ensure that connections don't starve if one connection
    /// always returns something. It is used in the `Stream` implementation of
    /// `ConnectionManager`.
    poll_index: usize,
    /// The task of the current `Run`, which polls all connections.
    ///
    /// It will be notified when a new connection is added.
    task: Option<Task>,
}

impl ConnectionManager {
    /// Creates a new `ConnectionManager` which is then used to add new
    /// connections.
    ///
    /// Connecting to a server is done by [`ConnectionManager::add_connection`].
    ///
    /// # Example
    ///
    /// ```
    /// # extern crate tokio_core;
    /// # extern crate tsclientlib;
    /// # use std::boxed::Box;
    /// # use std::error::Error;
    /// #
    /// # use tsclientlib::ConnectionManager;
    /// # fn main() {
    /// #
    /// let core = tokio_core::reactor::Core::new().unwrap();
    /// let cm = ConnectionManager::new(core.handle());
    /// # }
    /// ```
    ///
    /// [`ConnectionManager::add_connection`]: #method.add_connection
    pub fn new(handle: Handle) -> Self {
        // Initialize tsproto if it was not done yet
        static TSPROTO_INIT: Once = ONCE_INIT;
        TSPROTO_INIT.call_once(|| tsproto::init()
            .expect("tsproto failed to initialize"));

        // TODO Create with builder so the logger is optional
        // Don't log anything to console as default setting
        // Option to log to a file
        let logger = {
            let decorator = slog_term::TermDecorator::new().build();
            let drain = slog_term::FullFormat::new(decorator).build().fuse();
            let drain = slog_async::Async::new(drain).build().fuse();

            slog::Logger::root(drain, o!())
        };

        Self {
            inner: Rc::new(RefCell::new(InnerCM {
                handle,
                logger,
                connections: Map::new(),
            })),
            poll_index: 0,
            task: None,
        }
    }

    /// Connect to a server.
    pub fn add_connection(&mut self, mut config: ConnectOptions) -> Connect {
        let res: BoxFuture<_>;
        {
            let inner = self.inner.borrow();
            let addr = config.address.expect(
                "Invalid ConnectOptions, this should not happen");
            let private_key = match config.private_key.take().map(Ok)
                .unwrap_or_else(|| {
                    // Create new ECDH key
                    crypto::EccKeyPrivP256::create()
                }) {
                Ok(key) => key,
                Err(error) => return Connect::new_from_error(error.into()),
            };

            let client = match client::ClientData::new(
                config.local_address,
                private_key,
                inner.handle.clone(),
                true,
                tsproto::connectionmanager::SocketConnectionManager::new(),
                None,
            ) {
                Ok(client) => client,
                Err(error) => return Connect::new_from_error(error.into()),
            };

            // Set the data reference
            {
                let c2 = client.clone();
                let mut client = client.borrow_mut();
                client.connection_manager.set_data_ref(Rc::downgrade(&c2));
            }
            client::default_setup(&client, config.log_packets);

            // Create a connection
            let connect_fut = client::connect(&client, addr);

            let logger = inner.logger.clone();
            let inner = Rc::downgrade(&self.inner);
            let client2 = client.clone();

            // Poll the connection for packets
            let initserver_poll = client::ClientData::get_packets(
                Rc::downgrade(&client))
                .filter_map(|(_, p)| {
                    // Filter commands
                    if let Packet { data: packets::Data::Command(cmd), .. } = p {
                        Some(cmd)
                    } else {
                        None
                    }
                })
                .into_future().map_err(|(e, _)| e.into())
                .and_then(move |(cmd, _)| -> BoxFuture<_> {
                    let cmd = if let Some(cmd) = cmd {
                        cmd
                    } else {
                        return Box::new(future::err(Error::ConnectionFailed(
                            String::from("Connection ended"))));
                    };

                    let cmd = cmd.get_commands().remove(0);
                    let notif = tryf!(messages::Message::parse(cmd));
                    if let messages::Message::InitServer(p) = notif {
                        // Create a connection id
                        let inner = inner.upgrade().expect(
                            "Connection manager does not exist anymore");
                        let mut inner = inner.borrow_mut();
                        let id = inner.find_connection_id();

                        let con;
                        {
                            let mut client = client2.borrow_mut();
                            con = client.connection_manager
                                .get_connection(addr).unwrap();
                        }

                        // Create the connection
                        let con = structs::NetworkWrapper::new(id, client2,
                            Rc::downgrade(&con), &p);

                        // Add the connection
                        inner.connections.insert(id, con);

                        Box::new(future::ok(id))
                    } else {
                        Box::new(future::err(Error::ConnectionFailed(
                            String::from("Got no initserver"))))
                    }
                });

            res = Box::new(connect_fut.and_then(move |()| {
                // TODO Add possibility to specify offset and level in ConnectOptions
                // Compute hash cash
                let mut time_reporter = slog_perf::TimeReporter::new_with_level(
                    "Compute public key hash cash level", logger.clone(),
                    slog::Level::Info);
                time_reporter.start("Compute public key hash cash level");
                let (offset, omega) = {
                    let mut c = client.borrow_mut();
                    let pub_k = c.private_key.to_pub();
                    (algs::hash_cash(&pub_k, 8).unwrap(),
                    pub_k.to_ts().unwrap())
                };
                time_reporter.finish();
                info!(logger, "Computed hash cash level";
                    "level" => algs::get_hash_cash_level(&omega, offset),
                    "offset" => offset);

                // Create clientinit packet
                let header = Header::new(PacketType::Command);
                let mut command = commands::Command::new("clientinit");
                command.push("client_nickname", config.name);
                command.push("client_version", config.version.get_version_string());
                command.push("client_platform", config.version.get_platform());
                command.push("client_input_hardware", "1");
                command.push("client_output_hardware", "1");
                command.push("client_default_channel", "");
                command.push("client_default_channel_password", "");
                command.push("client_server_password", "");
                command.push("client_meta_data", "");
                command.push("client_version_sign", base64::encode(
                    config.version.get_signature()));
                command.push("client_key_offset", offset.to_string());
                command.push("client_nickname_phonetic", "");
                command.push("client_default_token", "");
                command.push("hwid", "123,456");
                let p_data = packets::Data::Command(command);
                let clientinit_packet = Packet::new(header, p_data);

                let sink = Data::get_packets(Rc::downgrade(&client));

                sink.send((addr, clientinit_packet))
            })
            .map_err(|e| e.into())
            // Wait until we sent the clientinit packet and afterwards received
            // the initserver packet.
            .join(initserver_poll)
            .map(|(_, id)| id));
        }
        Connect::new_from_future(self.run().select2(res))
    }

    /// Disconnect from a server.
    ///
    /// # Arguments
    /// - `id`: The connection which should be removed.
    /// - `options`: Either `None` or `DisconnectOptions`.
    ///
    /// # Examples
    ///
    /// Use default options:
    ///
    /// ```rust,no_run
    /// # extern crate tokio_core;
    /// # extern crate tsclientlib;
    /// # use std::boxed::Box;
    /// #
    /// # use tsclientlib::{ConnectionId, ConnectionManager};
    /// # fn main() {
    /// #
    /// let mut core = tokio_core::reactor::Core::new().unwrap();
    /// let mut cm = ConnectionManager::new(core.handle());
    ///
    /// // Add connection...
    ///
    /// # let con_id = ConnectionId(0);
    /// let disconnect_future = cm.remove_connection(con_id, None);
    /// core.run(disconnect_future).unwrap();
    /// # }
    /// ```
    ///
    /// Specify a reason and a quit message:
    ///
    /// ```rust,no_run
    /// # extern crate tokio_core;
    /// # extern crate tsclientlib;
    /// # use std::boxed::Box;
    /// #
    /// # use tsclientlib::{ConnectionId, ConnectionManager, DisconnectOptions,
    /// # Reason};
    /// # fn main() {
    /// #
    /// # let mut core = tokio_core::reactor::Core::new().unwrap();
    /// # let mut cm = ConnectionManager::new(core.handle());
    /// # let con_id = ConnectionId(0);
    /// cm.remove_connection(con_id, DisconnectOptions::new()
    ///     .reason(Reason::Clientdisconnect)
    ///     .message("Away for a while"));
    /// # }
    /// ```
    pub fn remove_connection<O: Into<Option<DisconnectOptions>>>(&mut self,
        id: ConnectionId, options: O) -> Disconnect {
        let client_con;
        let client_data;
        {
            let inner_b = self.inner.borrow();
            if let Some(con) = inner_b.connections.get(&id) {
                client_con = con.client_connection.clone();
                client_data = con.client_data.clone();
            } else {
                return Disconnect::new_from_ok();
            }
        }

        let client_con = if let Some(c) = client_con.upgrade() {
            c
        } else {
            // Already disconnected
            return Disconnect::new_from_ok();
        };

        let header = Header::new(PacketType::Command);
        let mut command = commands::Command::new("clientdisconnect");

        let options = options.into().unwrap_or_default();
        if let Some(reason) = options.reason {
            command.push("reasonid", (reason as u8).to_string());
        }
        if let Some(msg) = options.message {
            command.push("reasonmsg", msg);
        }

        let p_data = packets::Data::Command(command);
        let packet = Packet::new(header, p_data);

        let addr;
        {
            let mut con = client_con.borrow_mut();
            con.resender.handle_event(ResenderEvent::Disconnecting);
            addr = con.address;
        }

        let sink = Data::get_packets(Rc::downgrade(&client_data));
        let wait_for_state = client::wait_for_state(&client_data, addr, |state| {
            if let client::ServerConnectionState::Disconnected = *state {
                true
            } else {
                false
            }
        });
        let fut: BoxFuture<_> = Box::new(sink.send((addr, packet))
            .and_then(move |_| wait_for_state)
            .map_err(|e| e.into()));
        Disconnect::new_from_future(self.run().select(fut))
    }

    #[inline]
    pub fn get_connection(&self, id: ConnectionId) -> Option<Connection> {
        if self.inner.borrow().connections.contains_key(&id) {
            Some(Connection { cm: self, id })
        } else {
            None
        }
    }

    #[inline]
    pub fn get_mut_connection(&mut self, id: ConnectionId) -> Option<ConnectionMut> {
        if self.inner.borrow().connections.contains_key(&id) {
            Some(ConnectionMut { cm: self, id })
        } else {
            None
        }
    }

    #[inline]
    /// Creates a future to handle all packets.
    pub fn run(&mut self) -> Run {
        Run { cm: self }
    }
}

// Private methods
impl ConnectionManager {
    fn get_file(&self, _con: ConnectionId, _chan: ChannelId, _path: &str, _file: &str) -> Ref<structs::File> {
        unimplemented!("File transfer is not yet implemented")
    }

    fn get_chat_entry(&self, _con: ConnectionId, _sender: ClientId) -> Ref<structs::ChatEntry> {
        unimplemented!("Chatting is not yet implemented")
    }

    /// Poll like a stream to get the next message.
    fn poll_stream(&mut self) -> futures::Poll<Option<(ConnectionId, Message)>,
        Error> {
        if !self.task.as_ref().map(|t| t.will_notify_current()).unwrap_or(false) {
            self.task = Some(task::current());
        }

        // Poll all connections
        let inner = &mut *self.inner.borrow_mut();
        let keys: Vec<_> = inner.connections.keys().cloned().collect();
        if keys.is_empty() {
            // Wait until our task gets notified
            return Ok(futures::Async::NotReady);
        }

        if self.poll_index >= keys.len() {
            self.poll_index = 0;
        }

        let mut remove_connection = false;
        let mut result = Ok(futures::Async::NotReady);
        for con_id in 0..keys.len() {
            let i = (self.poll_index + con_id) % keys.len();
            let con = inner.connections.get_mut(&keys[i]).unwrap();
            match con.poll() {
                Ok(futures::Async::Ready(None)) =>
                    warn!(inner.logger, "Got None from a connection";
                        "connection" => %keys[i].0),
                Ok(futures::Async::Ready(Some((_, res)))) => {
                    // Check if the connection is still alive
                    if con.client_connection.upgrade().is_none() {
                        remove_connection = true;
                    }
                    self.poll_index = i + 1;
                    result = Ok(futures::Async::Ready(Some((keys[i], res))));
                    break;
                }
                Ok(futures::Async::NotReady) => {}
                Err(error) =>
                    warn!(inner.logger, "Got an error from a connection";
                        "error" => ?error, "connection" => %keys[i].0),
            }
        }
        if remove_connection {
            // Remove the connection
            inner.connections.remove(&keys[self.poll_index - 1]);
        }
        result
    }

    #[inline]
    // Poll like a future created by (ConnectionManager as Stream).for_each().
    fn poll_future(&mut self) -> futures::Poll<(), Error> {
        loop {
            match self.poll_stream()? {
                futures::Async::Ready(Some(_)) => {}
                futures::Async::Ready(None) =>
                    return Ok(futures::Async::Ready(())),
                futures::Async::NotReady => return Ok(futures::Async::NotReady),
            }
        }
    }
}

impl fmt::Debug for ConnectionManager {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "ConnectionManager(...)")
    }
}

/// A future which runs the `ConnectionManager` indefinitely.
#[derive(Debug)]
pub struct Run<'a> {
    cm: &'a mut ConnectionManager,
}

impl<'a> Future for Run<'a> {
    type Item = ();
    type Error = Error;

    #[inline]
    fn poll(&mut self) -> futures::Poll<Self::Item, Self::Error> {
        self.cm.poll_future()
    }
}

pub struct Connect<'a> {
    /// Contains an error if the `add_connection` functions should return an
    /// error.
    inner: Either<Option<Error>,
        futures::future::Select2<Run<'a>, BoxFuture<ConnectionId>>>,
}

impl<'a> Connect<'a> {
    fn new_from_error(error: Error) -> Self {
        Self { inner: Either::A(Some(error)) }
    }

    fn new_from_future(future: futures::future::Select2<Run<'a>,
        BoxFuture<ConnectionId>>) -> Self {
        Self { inner: Either::B(future) }
    }
}

impl<'a> Future for Connect<'a> {
    type Item = ConnectionId;
    type Error = Error;

    fn poll(&mut self) -> futures::Poll<Self::Item, Self::Error> {
        match self.inner {
            // Take the error, this will panic if called twice
            Either::A(ref mut error) => Err(error.take().unwrap()),
            Either::B(ref mut inner) => match inner.poll() {
                Ok(futures::Async::Ready(Either::A(((), _)))) =>
                    Err(format_err!("Could not connect").into()),
                Ok(futures::Async::Ready(Either::B((id, _)))) =>
                    Ok(futures::Async::Ready(id)),
                Ok(futures::Async::NotReady) => Ok(futures::Async::NotReady),
                Err(Either::A((error, _))) |
                Err(Either::B((error, _))) => Err(error),
            }
        }
    }
}

pub struct Disconnect<'a> {
    inner: Option<futures::future::Select<Run<'a>, BoxFuture<()>>>,
}

impl<'a> Disconnect<'a> {
    fn new_from_ok() -> Self {
        Self { inner: None }
    }

    fn new_from_future(future: futures::future::Select<Run<'a>, BoxFuture<()>>)
        -> Self {
        Self { inner: Some(future) }
    }
}

impl<'a> Future for Disconnect<'a> {
    type Item = ();
    type Error = Error;

    #[inline]
    fn poll(&mut self) -> futures::Poll<Self::Item, Self::Error> {
        match self.inner {
            None => Ok(futures::Async::Ready(())),
            Some(ref mut f) => f.poll().map(|r| match r {
                futures::Async::Ready(_) => futures::Async::Ready(()),
                futures::Async::NotReady => futures::Async::NotReady,
            }).map_err(|(e, _)| e),
        }
    }
}

impl<'a> Connection<'a> {
    #[inline]
    pub fn get_server(&self) -> Server {
        Server {
            cm: self.cm,
            connection_id: self.id,
        }
    }
}

impl<'a> ConnectionMut<'a> {
    #[inline]
    pub fn get_server(&self) -> Server {
        Server {
            cm: self.cm,
            connection_id: self.id,
        }
    }

    #[inline]
    pub fn get_mut_server(&mut self) -> ServerMut {
        ServerMut {
            cm: self.cm,
            connection_id: self.id,
        }
    }
}

/// The configuration used to create a new connection.
///
/// This is a builder for a connection.
///
/// # Example
///
/// ```rust,no_run
/// # extern crate tokio_core;
/// # extern crate tsclientlib;
/// # use std::boxed::Box;
/// #
/// # use tsclientlib::{ConnectionManager, ConnectOptions};
/// # fn main() {
/// #
/// let mut core = tokio_core::reactor::Core::new().unwrap();
///
/// let addr: std::net::SocketAddr = "127.0.0.1:9987".parse().unwrap();
/// let con_config = ConnectOptions::from_address(addr);
///
/// let mut cm = ConnectionManager::new(core.handle());
/// let con = core.run(cm.add_connection(con_config)).unwrap();
/// # }
/// ```
#[derive(Debug)]
pub struct ConnectOptions {
    address: Option<SocketAddr>,
    local_address: SocketAddr,
    private_key: Option<crypto::EccKeyPrivP256>,
    name: String,
    version: Version,
    log_packets: bool,
}

impl ConnectOptions {
    /// A private method to create a config with only default values.
    ///
    /// This is not in the public interface because the created configuration
    /// is invalid.
    #[inline]
    fn default() -> Self {
        Self {
            address: None,
            local_address: "0.0.0.0:0".parse().unwrap(),
            private_key: None,
            name: String::from("TeamSpeakUser"),
            version: Version::Linux_3_1_8,
            log_packets: false,
        }
    }

    /// Start creating the configuration of a new connection.
    ///
    /// The address of the server has to be supplied.
    #[inline]
    pub fn from_address(address: SocketAddr) -> Self {
        Self {
            address: Some(address),
            .. Self::default()
        }
    }

    /// The address for the socket of our client
    ///
    /// # Default
    ///
    /// 0.0.0.0:0
    #[inline]
    pub fn local_address(mut self, local_address: SocketAddr) -> Self {
        self.local_address = local_address;
        self
    }

    /// Set the private key of the user.
    ///
    /// # Default
    ///
    /// A new identity is generated when connecting.
    #[inline]
    pub fn private_key(mut self, private_key: crypto::EccKeyPrivP256)
        -> Self {
        self.private_key = Some(private_key);
        self
    }

    /// Takes the private key as encoded by TeamSpeak (libtomcrypt export and
    /// base64 encoded).
    ///
    /// # Default
    ///
    /// A new identity is generated when connecting.
    ///
    /// # Error
    ///
    /// An error is returned if either the string is not encoded in valid base64
    /// or libtomcrypt cannot import the key.
    #[inline]
    pub fn private_key_ts(mut self, private_key: &str) -> Result<Self> {
        self.private_key = Some(crypto::EccKeyPrivP256::from_ts(private_key)?);
        Ok(self)
    }

    /// The name of the user.
    ///
    /// # Default
    ///
    /// TeamSpeakUser
    #[inline]
    pub fn name(mut self, name: String) -> Self {
        self.name = name;
        self
    }

    /// The displayed version of the client.
    ///
    /// # Default
    ///
    /// 3.1.8 on Linux
    #[inline]
    pub fn version(mut self, version: Version) -> Self {
        self.version = version;
        self
    }

    /// If the content of all packets in high-level and byte-array form should
    /// be written to the logger.
    ///
    /// # Default
    ///
    /// false
    #[inline]
    pub fn log_packets(mut self, log_packets: bool) -> Self {
        self.log_packets = log_packets;
        self
    }
}

pub struct DisconnectOptions {
    reason: Option<Reason>,
    message: Option<String>,
}

impl Default for DisconnectOptions {
    #[inline]
    fn default() -> Self {
        Self {
            reason: None,
            message: None,
        }
    }
}

impl DisconnectOptions {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the reason for leaving.
    ///
    /// # Default
    ///
    /// None
    #[inline]
    pub fn reason(mut self, reason: Reason) -> Self {
        self.reason = Some(reason);
        self
    }

    /// Set the leave message.
    ///
    /// You also have to set the reason, otherwise the message will not be
    /// displayed.
    ///
    /// # Default
    ///
    /// None
    #[inline]
    pub fn message<S: Into<String>>(mut self, message: S) -> Self {
        self.message = Some(message.into());
        self
    }
}
