//! tsclientlib is a library which makes it simple to create TeamSpeak clients
//! and bots.
//!
//! If you want a full client application, you might want to have a look at
//! [Qint].
//!
//! The base class of this library is the [`Connection`]. One instance of this
//! struct manages a single connection to a server.
//!
//! The futures from this library **must** be run in a tokio threadpool, so they
//! can use `tokio_threadpool::blocking`.
//!
//! [`Connection`]: struct.Connection.html
//! [Qint]: https://github.com/ReSpeak/Qint

#![allow(dead_code)] // TODO

extern crate base64;
extern crate bytes;
extern crate chashmap;
extern crate chrono;
#[macro_use]
extern crate failure;
extern crate futures;
extern crate num;
extern crate rand;
extern crate reqwest;
#[macro_use]
extern crate slog;
extern crate slog_async;
extern crate slog_perf;
extern crate slog_term;
extern crate tokio;
extern crate tokio_threadpool;
extern crate trust_dns_proto;
extern crate trust_dns_resolver;
extern crate tsproto;
extern crate tsproto_commands;

use std::fmt;
use std::net::SocketAddr;
use std::ops::Deref;
use std::sync::{Arc, Mutex, MutexGuard, Once, ONCE_INIT};

use chrono::{DateTime, Utc};
use failure::ResultExt;
use futures::{future, Future, Sink, stream, Stream};
use futures::sync::mpsc;
use slog::{Drain, Logger};
use tsproto::algorithms as algs;
use tsproto::{client, crypto, packets, commands};
use tsproto::commands::Command;
use tsproto::packets::{Header, Packet, PacketType};
use tsproto_commands::messages::Message;

use packet_handler::{ReturnCodeHandler, SimplePacketHandler};

macro_rules! copy_attrs {
    ($from:ident, $to:ident; $($attr:ident),* $(,)*; $($extra:ident: $ex:expr),* $(,)*) => {
        $to {
            $($attr: $from.$attr.clone(),)*
            $($extra: $ex,)*
        }
    };
}

mod packet_handler;
pub mod data;
pub mod resolver;

// Reexports
pub use tsproto_commands::{messages, Reason, Uid};
pub use tsproto_commands::versions::Version;
pub use tsproto_commands::errors::Error as TsError;

type BoxFuture<T> = Box<Future<Item = T, Error = Error> + Send>;
type Result<T> = std::result::Result<T, Error>;

#[derive(Fail, Debug)]
pub enum Error {
    #[fail(display = "{}", _0)]
    Base64(#[cause] base64::DecodeError),
    #[fail(display = "{}", _0)]
    Canceled(#[cause] futures::Canceled),
    #[fail(display = "{}", _0)]
    DnsProto(#[cause] trust_dns_proto::error::ProtoError),
    #[fail(display = "{}", _0)]
    Io(#[cause] std::io::Error),
    #[fail(display = "{}", _0)]
    ParseMessage(#[cause] tsproto_commands::messages::ParseError),
    #[fail(display = "{}", _0)]
    Resolve(#[cause] trust_dns_resolver::error::ResolveError),
    #[fail(display = "{}", _0)]
    Reqwest(#[cause] reqwest::Error),
    #[fail(display = "{}", _0)]
    Ts(#[cause] TsError),
    #[fail(display = "{}", _0)]
    Tsproto(#[cause] tsproto::Error),
    #[fail(display = "{}", _0)]
    Utf8(#[cause] std::str::Utf8Error),
    #[fail(display = "{}", _0)]
    Other(#[cause] failure::Compat<failure::Error>),

    #[fail(display = "Connection failed ({})", _0)]
    ConnectionFailed(String),

    #[doc(hidden)]
    #[fail(display = "Nonexhaustive enum – not an error")]
    __Nonexhaustive,
}

impl From<base64::DecodeError> for Error {
    fn from(e: base64::DecodeError) -> Self {
        Error::Base64(e)
    }
}

impl From<futures::Canceled> for Error {
    fn from(e: futures::Canceled) -> Self {
        Error::Canceled(e)
    }
}

impl From<trust_dns_proto::error::ProtoError> for Error {
    fn from(e: trust_dns_proto::error::ProtoError) -> Self {
        Error::DnsProto(e)
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

impl From<tsproto_commands::messages::ParseError> for Error {
    fn from(e: tsproto_commands::messages::ParseError) -> Self {
        Error::ParseMessage(e)
    }
}

impl From<trust_dns_resolver::error::ResolveError> for Error {
    fn from(e: trust_dns_resolver::error::ResolveError) -> Self {
        Error::Resolve(e)
    }
}

impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        Error::Reqwest(e)
    }
}

impl From<TsError> for Error {
    fn from(e: TsError) -> Self {
        Error::Ts(e)
    }
}

impl From<tsproto::Error> for Error {
    fn from(e: tsproto::Error) -> Self {
        Error::Tsproto(e)
    }
}

impl From<std::str::Utf8Error> for Error {
    fn from(e: std::str::Utf8Error) -> Self {
        Error::Utf8(e)
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

type PHBox = Box<PacketHandler + Send>;
pub trait PacketHandler {
    fn new_connection(
        &mut self,
        command_stream: Box<Stream<Item=Packet, Error=tsproto::Error> + Send>,
        audio_stream: Box<Stream<Item=Packet, Error=tsproto::Error> + Send>,
    );
    /// Clone into a box.
    fn clone(&self) -> PHBox;
}

pub struct ConnectionLock<'a> {
    guard: MutexGuard<'a, data::Connection>,
}

impl<'a> Deref for ConnectionLock<'a> {
    type Target = data::Connection;

    fn deref(&self) -> &Self::Target {
        &*self.guard
    }
}

#[derive(Clone)]
struct InnerConnection {
    connection: Arc<Mutex<data::Connection>>,
    client_data: client::ClientDataM<SimplePacketHandler>,
    client_connection: client::ClientConVal,
    return_code_handler: Arc<ReturnCodeHandler>,
}

#[derive(Clone)]
pub struct Connection {
    inner: InnerConnection,
}

/// The main type of this crate, which represents a connection to a server.
///
/// A new connection can be opened with the [`Connection::new`] function.
///
/// # Examples
/// This will open a connection to the TeamSpeak server at `localhost`.
///
/// ```no_run
/// extern crate tokio;
/// extern crate tsclientlib;
///
/// use tokio::prelude::Future;
/// use tsclientlib::{Connection, ConnectOptions};
///
/// fn main() {
///     tokio::run(
///         Connection::new(ConnectOptions::new("localhost"))
///         .map(|connection| ())
///         .map_err(|_| ())
///     );
/// }
/// ```
///
/// [`Connection::new`]: #method.new
impl Connection {
    /// Connect to a server.
    ///
    /// This function opens a new connection to a server. The returned future
    /// resolves, when the connection is established successfully.
    ///
    /// Settings like nickname of the user can be set using the
    /// [`ConnectOptions`] parameter.
    ///
    /// # Examples
    /// This will open a connection to the TeamSpeak server at `localhost`.
    ///
    /// ```no_run
    /// # extern crate tokio;
    /// # extern crate tsclientlib;
    /// # use tokio::prelude::Future;
    /// # use tsclientlib::{Connection, ConnectOptions};
    /// #
    /// # fn main() {
    ///     tokio::run(
    ///         Connection::new(ConnectOptions::new("localhost"))
    ///         .map(|connection| ())
    ///         .map_err(|_| ())
    ///     );
    /// # }
    /// ```
    ///
    /// [`ConnectOptions`]: struct.ConnectOptions.html
    pub fn new(mut options: ConnectOptions) -> BoxFuture<Connection> {
        // Initialize tsproto if it was not done yet
        static TSPROTO_INIT: Once = ONCE_INIT;
        TSPROTO_INIT.call_once(|| tsproto::init()
            .expect("tsproto failed to initialize"));

        let logger = options.logger.take().unwrap_or_else(|| {
            let decorator = slog_term::TermDecorator::new().build();
            let drain = slog_term::FullFormat::new(decorator).build().fuse();
            let drain = slog_async::Async::new(drain).build().fuse();

            slog::Logger::root(drain, o!())
        });
        let logger = logger.new(o!("addr" => options.address.to_string()));

        // Try all addresses
        let addr: Box<Stream<Item=_, Error=_> + Send> = options.address.resolve(&logger);
        let private_key = match options.private_key.take().map(Ok)
            .unwrap_or_else(|| {
                // Create new ECDH key
                crypto::EccKeyPrivP256::create()
            }) {
            Ok(key) => key,
            Err(e) => return Box::new(future::err(e.into())),
        };

        let logger2 = logger.clone();
        Box::new(addr.and_then(move |addr| -> Box<Future<Item=_, Error=_> + Send> {
            let log_config = tsproto::handler_data::LogConfig::new(
                options.log_packets, options.log_packets);
            let mut packet_handler = SimplePacketHandler::new(logger.clone());
            let return_code_handler = packet_handler.return_codes.clone();
            let (initserver_send, initserver_recv) = mpsc::channel(0);
            packet_handler.initserver_sender = Some(initserver_send);
            if let Some(h) = &options.handle_packets {
                packet_handler.handle_packets = Some(h.as_ref().clone());
            }
            let packet_handler = client::DefaultPacketHandler::new(
                packet_handler);
            let client = match client::ClientData::new(
                options.local_address.unwrap_or_else(|| if addr.is_ipv4() {
                    "0.0.0.0:0".parse().unwrap()
                } else {
                    "[::]:0".parse().unwrap()
                }),
                private_key.clone(),
                true,
                None,
                packet_handler,
                tsproto::connectionmanager::SocketConnectionManager::new(),
                logger.clone(),
                log_config,
            ) {
                Ok(client) => client,
                Err(error) => return Box::new(future::err(error.into())),
            };

            // Set the data reference
            let client2 = Arc::downgrade(&client);
            client.try_lock().unwrap().packet_handler.complete(client2);

            let logger = logger.clone();
            let client = client.clone();
            let client2 = client.clone();
            let options = options.clone();

            // Create a connection
            debug!(logger, "Connecting"; "address" => %addr);
            let connect_fut = client::connect(Arc::downgrade(&client),
                &mut *client.lock().unwrap(), addr).from_err();

            let initserver_poll = initserver_recv.into_future()
                .map_err(|e| format_err!("Error while waiting for initserver \
                    ({:?})", e).into())
                .and_then(move |(cmd, _)| {
                    let cmd = match cmd {
                        Some(c) => c,
                        None => return Err(Error::ConnectionFailed(
                            String::from("Got no initserver"))),
                    };
                    let cmd = cmd.get_commands().remove(0);
                    let notif = Message::parse(cmd)?;
                    if let Message::InitServer(p) = notif {
                        Ok(p)
                    } else {
                        Err(Error::ConnectionFailed(
                            String::from("Got no initserver")))
                    }
                });

            let logger2 = logger.clone();
            Box::new(connect_fut
                .and_then(move |con| {
                    // TODO Add possibility to specify offset and level in ConnectOptions
                    // Compute hash cash
                    let mut time_reporter = slog_perf::TimeReporter::new_with_level(
                        "Compute public key hash cash level", logger2.clone(),
                        slog::Level::Info);
                    time_reporter.start("Compute public key hash cash level");
                    let pub_k = {
                        let mut c = client.lock().unwrap();
                        c.private_key.to_pub()
                    };
                    future::poll_fn(move || {
                        tokio_threadpool::blocking(|| {
                            let res = (con.clone(), algs::hash_cash(&pub_k, 8).unwrap(),
                                pub_k.to_ts().unwrap());
                            res
                        })
                    }).map(|r| { time_reporter.finish(); r })
                    .map_err(|e| format_err!("Failed to start \
                        blocking operation ({:?})", e).into())
                })
                .and_then(move |(con, offset, omega)| {
                    info!(logger, "Computed hash cash level";
                        "level" => algs::get_hash_cash_level(&omega, offset),
                        "offset" => offset);

                    // Create clientinit packet
                    let header = Header::new(PacketType::Command);
                    let mut command = commands::Command::new("clientinit");
                    command.push("client_nickname", options.name);
                    command.push("client_version", options.version.get_version_string());
                    command.push("client_platform", options.version.get_platform());
                    command.push("client_input_hardware", "1");
                    command.push("client_output_hardware", "1");
                    command.push("client_default_channel", "");
                    command.push("client_default_channel_password", "");
                    command.push("client_server_password", "");
                    command.push("client_meta_data", "");
                    command.push("client_version_sign", base64::encode(
                        options.version.get_signature()));
                    command.push("client_key_offset", offset.to_string());
                    command.push("client_nickname_phonetic", "");
                    command.push("client_default_token", "");
                    command.push("hwid", "123,456");
                    let p_data = packets::Data::Command(command);
                    let clientinit_packet = Packet::new(header, p_data);

                    let sink = con.as_packet_sink();

                    sink.send(clientinit_packet).map(move |_| con)
                })
                .from_err()
                // Wait until we sent the clientinit packet and afterwards received
                // the initserver packet.
                .and_then(move |con| initserver_poll.map(|r| (con, r)))
                .and_then(move |(con, initserver)| {
                    // Get uid of server
                    let uid = {
                        let mutex = con.upgrade().ok_or_else(||
                            format_err!("Connection does not exist anymore"))?
                            .mutex;
                        let con = mutex.lock().unwrap();
                        con.1.params.as_ref().ok_or_else(||
                            format_err!("Connection params do not exist"))?
                            .public_key.get_uid()?
                    };

                    // Create connection
                    let data = data::Connection::new(Uid(uid),
                        &initserver);
                    let con = InnerConnection {
                        connection: Arc::new(Mutex::new(data)),
                        client_data: client2,
                        client_connection: con,
                        return_code_handler,
                    };
                    Ok(Connection { inner: con })
                }))
        })
        .then(move |r| -> Result<_> {
            if let Err(e) = &r {
                debug!(logger2, "Connecting failed, trying next address";
                    "error" => ?e);
            }
            Ok(r.ok())
        })
        .filter_map(|r| r)
        .into_future()
        .map_err(|_| Error::from(format_err!("Failed to connect to server")))
        .and_then(|(r, _)| r.ok_or_else(|| format_err!("Failed to connect to server").into()))
        )
    }

    /// **This is part of the unstable interface.**
    ///
    /// You can use it if you need access to lower level functions, but this
    /// interface may change on any version changes.
    pub fn get_packet_sink(&self) -> impl Sink<SinkItem=Packet, SinkError=Error> {
        self.inner.client_connection.as_packet_sink().sink_map_err(|e| e.into())
    }

    /// **This is part of the unstable interface.**
    ///
    /// You can use it if you need access to lower level functions, but this
    /// interface may change on any version changes.
    pub fn get_udp_packet_sink(&self) -> impl Sink<SinkItem=(PacketType, u16, bytes::Bytes), SinkError=Error> {
        self.inner.client_connection.as_udp_packet_sink().sink_map_err(|e| e.into())
    }

    /// **This is part of the unstable interface.**
    ///
    /// You can use it if you need access to lower level functions, but this
    /// interface may change on any version changes.
    ///
    /// Adds a `return_code` to the command and returns if the corresponding
    /// answer is received. If an error occurs, the future will return an error.
    pub fn send_message(&self, msg: Message) -> impl Future<Item=(), Error=Error> {
        // Store waiting in HashMap<usize (return code), oneshot::Sender>
        // The packet handler then sends a result to the sender if the answer is
        // received.

        let np = msg.get_newprotocol();
        let typ = if !msg.get_commandlow() { PacketType::Command }
            else { PacketType::CommandLow };
        let mut cmd: Command = msg.into();
        let (code, recv) = self.inner.return_code_handler.get_return_code();
        cmd.push("return_code", code.to_string());
        let mut header = Header::new(typ);
        header.set_newprotocol(np);
        let data = if typ == PacketType::Command { packets::Data::Command(cmd) }
            else { packets::Data::CommandLow(cmd) };
        let packet = packets::Packet::new(header, data);

        // Send a message and wait until we get an answer for the return code
        self.get_packet_sink().send(packet).and_then(|_| recv
            .map_err(|e| format_err!("Too many return codes ({:?})", e).into()))
            .and_then(|r| if r == TsError::Ok { Ok(()) } else { Err(r.into()) })
    }

    pub fn lock(&self) -> ConnectionLock {
        ConnectionLock::new(self.inner.connection.lock().unwrap())
    }

    pub fn to_mut<'a>(&self, con: &'a data::Connection)
        -> data::ConnectionMut<'a> {
        data::ConnectionMut {
            connection: self.inner.clone(),
            inner: &con,
        }
    }

    /// Disconnect from the server.
    ///
    /// # Arguments
    /// - `options`: Either `None` or `DisconnectOptions`.
    ///
    /// # Examples
    ///
    /// Use default options:
    ///
    /// ```no_run
    /// # extern crate tokio;
    /// # extern crate tsclientlib;
    /// #
    /// # use tokio::prelude::Future;
    /// # use tsclientlib::{Connection, ConnectOptions};
    /// # fn main() {
    /// #
    /// tokio::run(Connection::new(ConnectOptions::new("localhost"))
    ///     .and_then(|connection| {
    ///         connection.disconnect(None)
    ///     })
    ///     .map_err(|_| ())
    /// );
    /// # }
    /// ```
    ///
    /// Specify a reason and a quit message:
    ///
    /// ```no_run
    /// # extern crate tokio;
    /// # extern crate tsclientlib;
    /// #
    /// # use tokio::prelude::Future;
    /// # use tsclientlib::{Connection, ConnectOptions};
    /// # fn main() {
    /// #
    /// use tsclientlib::{DisconnectOptions, Reason};
    /// tokio::run(Connection::new(ConnectOptions::new("localhost"))
    ///     .and_then(|connection| {
    ///         let options = DisconnectOptions::new()
    ///             .reason(Reason::Clientdisconnect)
    ///             .message("Away for a while");
    ///
    ///         connection.disconnect(options)
    ///     })
    ///     .map_err(|_| ())
    /// );
    /// # }
    /// ```
    pub fn disconnect<O: Into<Option<DisconnectOptions>>>(self, options: O)
        -> BoxFuture<()> {
        let options = options.into().unwrap_or_default();

        let header = Header::new(PacketType::Command);
        let mut command = commands::Command::new("clientdisconnect");

        if let Some(reason) = options.reason {
            command.push("reasonid", (reason as u8).to_string());
        }
        if let Some(msg) = options.message {
            command.push("reasonmsg", msg);
        }

        let p_data = packets::Data::Command(command);
        let packet = Packet::new(header, p_data);

        let wait_for_state = client::wait_for_state(&self.inner.client_connection, |state| {
            if let client::ServerConnectionState::Disconnected = state {
                true
            } else {
                false
            }
        });
        Box::new(self.inner.client_connection.as_packet_sink().send(packet)
            .and_then(move |_| wait_for_state)
            .from_err()
            .map(move |_| drop(self)))
    }
}

impl<'a> ConnectionLock<'a> {
    fn new(guard: MutexGuard<'a, data::Connection>) -> Self {
        Self { guard }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ServerAddress {
    SocketAddr(SocketAddr),
    Other(String),
}

impl From<SocketAddr> for ServerAddress {
    fn from(addr: SocketAddr) -> Self {
        ServerAddress::SocketAddr(addr)
    }
}

impl From<String> for ServerAddress {
    fn from(addr: String) -> Self {
        ServerAddress::Other(addr)
    }
}

impl<'a> From<&'a str> for ServerAddress {
    fn from(addr: &'a str) -> Self {
        ServerAddress::Other(addr.to_string())
    }
}

impl ServerAddress {
    pub fn resolve(&self, logger: &Logger) -> Box<Stream<Item=SocketAddr, Error=Error> + Send> {
        match self {
            ServerAddress::SocketAddr(a) => Box::new(stream::once(Ok(*a))),
            ServerAddress::Other(s) => Box::new(resolver::resolve(logger, s)),
        }
    }
}

impl fmt::Display for ServerAddress {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ServerAddress::SocketAddr(a) => fmt::Display::fmt(a, f),
            ServerAddress::Other(a) => fmt::Display::fmt(a, f),
        }
    }
}

/// The configuration to create a new connection.
///
/// # Example
///
/// ```no_run
/// # extern crate tokio;
/// # extern crate tsclientlib;
/// #
/// # use tokio::prelude::Future;
/// # use tsclientlib::{Connection, ConnectOptions};
/// # fn main() {
/// #
/// let con_config = ConnectOptions::new("localhost");
///
/// tokio::run(
///     Connection::new(con_config)
///     .map(|connection| ())
///     .map_err(|_| ())
/// );
/// # }
/// ```
pub struct ConnectOptions {
    address: ServerAddress,
    local_address: Option<SocketAddr>,
    private_key: Option<crypto::EccKeyPrivP256>,
    name: String,
    version: Version,
    logger: Option<Logger>,
    log_packets: bool,
    handle_packets: Option<PHBox>,
}

impl ConnectOptions {
    /// Start creating the configuration of a new connection.
    ///
    /// # Arguments
    /// The address of the server has to be supplied. The address can be a
    /// [`SocketAddr`], a [`String`] or directly a [`ServerAddress`]. A string
    /// will automatically be resolved from all formats supported by TeamSpeak.
    /// For details, see [`resolver::resolve`].
    ///
    /// [`SocketAddr`]: ../../std/net/enum.SocketAddr.html
    /// [`String`]: ../../std/string/struct.String.html
    /// [`ServerAddress`]: enum.ServerAddress.html
    /// [`resolver::resolve`]: resolver/method.resolve.html
    #[inline]
    pub fn new<A: Into<ServerAddress>>(address: A) -> Self {
        Self {
            address: address.into(),
            local_address: None,
            private_key: None,
            name: String::from("TeamSpeakUser"),
            version: Version::Linux_3_2_1,
            logger: None,
            log_packets: false,
            handle_packets: None,
        }
    }

    /// The address for the socket of our client
    ///
    /// # Default
    /// The default is `0.0.0:0` when connecting to an IPv4 address and `[::]:0`
    /// when connecting to an IPv6 address.
    #[inline]
    pub fn local_address(mut self, local_address: SocketAddr) -> Self {
        self.local_address = Some(local_address);
        self
    }

    /// Set the private key of the user.
    ///
    /// # Default
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
    /// A new identity is generated when connecting.
    ///
    /// # Error
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
    /// `TeamSpeakUser`
    #[inline]
    pub fn name(mut self, name: String) -> Self {
        self.name = name;
        self
    }

    /// The displayed version of the client.
    ///
    /// # Default
    /// `3.2.1 on Linux`
    #[inline]
    pub fn version(mut self, version: Version) -> Self {
        self.version = version;
        self
    }

    /// If the content of all packets in high-level and byte-array form should
    /// be written to the logger.
    ///
    /// # Default
    /// `false`
    #[inline]
    pub fn log_packets(mut self, log_packets: bool) -> Self {
        self.log_packets = log_packets;
        self
    }

    /// Set a custom logger for the connection.
    ///
    /// # Default
    /// A new logger is created.
    #[inline]
    pub fn logger(mut self, logger: Logger) -> Self {
        self.logger = Some(logger);
        self
    }

    /// Handle incomming command and audio packets in a custom way,
    /// additionally to the default handling.
    ///
    /// The given function will be called with a stream of command packets and a
    /// second stream of audio packets.
    ///
    /// # Default
    /// Packets are handled in the default way and then dropped.
    #[inline]
    pub fn handle_packets(mut self,
        handle_packets: PHBox) -> Self {
        self.handle_packets = Some(handle_packets);
        self
    }
}

impl fmt::Debug for ConnectOptions {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // Error if attributes are added
        let ConnectOptions {
            address, local_address, private_key, name, version, logger,
            log_packets, handle_packets: _,
        } = self;
        write!(f, "ConnectOptions {{ \
            address: {:?}, \
            local_address: {:?}, \
            private_key: {:?}, \
            name: {}, \
            version: {}, \
            logger: {:?}, \
            log_packets: {}, \
            }}", address, local_address, private_key, name, version, logger,
            log_packets)?;
        Ok(())
    }
}

impl Clone for ConnectOptions {
    fn clone(&self) -> Self {
        ConnectOptions {
            address: self.address.clone(),
            local_address: self.local_address.clone(),
            private_key: self.private_key.clone(),
            name: self.name.clone(),
            version: self.version.clone(),
            logger: self.logger.clone(),
            log_packets: self.log_packets.clone(),
            handle_packets: self.handle_packets.as_ref()
                .map(|h| h.as_ref().clone()),
        }
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
