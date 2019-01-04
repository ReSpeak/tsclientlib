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
// TODO Needed to call a Box<FnOnce>
#![feature(unsized_locals)]

#![recursion_limit="128"]

#[macro_use]
extern crate failure;

use std::fmt;
use std::net::SocketAddr;
use std::ops::Deref;
use std::sync::Arc;

use failure::ResultExt;
use futures::sync::oneshot;
use futures::{future, stream, Future, Sink, Stream};
use parking_lot::{Once, RwLock, RwLockReadGuard, ONCE_INIT};
use slog::{debug, error, info, o, Drain, Logger};
use tsproto::algorithms as algs;
use tsproto::{client, crypto, log};
use tsproto::connectionmanager::ConnectionManager;
use tsproto::handler_data::{ConnectionListener, ConnectionValue};
use tsproto::packets::{
	Direction, InAudio, InCommand, OutCommand, OutPacket, PacketType,
};
#[cfg(feature = "audio")]
use tsproto_audio::ts_to_audio::AudioPacketHandler;
use tsproto_commands::messages::s2c::{InMessage, InMessages};

use crate::packet_handler::{ReturnCodeHandler, SimplePacketHandler};

macro_rules! copy_attrs {
	($from:ident, $to:ident; $($attr:ident),* $(,)*; $($extra:ident: $ex:expr),* $(,)*) => {
		$to {
			$($attr: $from.$attr.into(),)*
			$($extra: $ex,)*
		}
	};
}

pub mod data;
mod packet_handler;
pub mod resolver;

// Reexports
pub use tsproto_commands::errors::Error as TsError;
pub use tsproto_commands::versions::Version;
pub use tsproto_commands::{
	messages, ChannelId, ClientId, Reason, ServerGroupId, Uid,
};

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
	ThreadpoolBlocking(#[cause] tokio_threadpool::BlockingError),
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
	fn from(e: base64::DecodeError) -> Self { Error::Base64(e) }
}

impl From<futures::Canceled> for Error {
	fn from(e: futures::Canceled) -> Self { Error::Canceled(e) }
}

impl From<trust_dns_proto::error::ProtoError> for Error {
	fn from(e: trust_dns_proto::error::ProtoError) -> Self {
		Error::DnsProto(e)
	}
}

impl From<std::io::Error> for Error {
	fn from(e: std::io::Error) -> Self { Error::Io(e) }
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
	fn from(e: reqwest::Error) -> Self { Error::Reqwest(e) }
}

impl From<tokio_threadpool::BlockingError> for Error {
	fn from(e: tokio_threadpool::BlockingError) -> Self {
		Error::ThreadpoolBlocking(e)
	}
}

impl From<TsError> for Error {
	fn from(e: TsError) -> Self { Error::Ts(e) }
}

impl From<tsproto::Error> for Error {
	fn from(e: tsproto::Error) -> Self { Error::Tsproto(e) }
}

impl From<std::str::Utf8Error> for Error {
	fn from(e: std::str::Utf8Error) -> Self { Error::Utf8(e) }
}

impl From<failure::Error> for Error {
	fn from(e: failure::Error) -> Self {
		let r: std::result::Result<(), _> = Err(e);
		Error::Other(r.compat().unwrap_err())
	}
}

pub type PHBox = Box<PacketHandler + Send + Sync>;
pub trait PacketHandler {
	fn new_connection(
		&mut self,
		command_stream: Box<
			Stream<Item = InCommand, Error = tsproto::Error> + Send,
		>,
		audio_stream: Box<
			Stream<Item = InAudio, Error = tsproto::Error> + Send,
		>,
	);
	/// Clone into a box.
	fn clone(&self) -> PHBox;
}

pub struct ConnectionLock<'a> {
	guard: RwLockReadGuard<'a, data::Connection>,
}

impl<'a> Deref for ConnectionLock<'a> {
	type Target = data::Connection;

	fn deref(&self) -> &Self::Target { &*self.guard }
}

#[derive(Clone)]
struct InnerConnection {
	connection: Arc<RwLock<data::Connection>>,
	client_data: client::ClientDataM<SimplePacketHandler>,
	client_connection: client::ClientConVal,
	return_code_handler: Arc<ReturnCodeHandler>,
}

#[derive(Clone)]
pub struct Connection {
	inner: InnerConnection,
}

struct DisconnectListener(Option<Box<FnOnce() + Send>>);

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
		TSPROTO_INIT.call_once(|| {
			tsproto::init().expect("tsproto failed to initialize")
		});
		#[cfg(feature = "audio")]
		tsproto_audio::init();

		let logger = options.logger.take().unwrap_or_else(|| {
			let decorator = slog_term::TermDecorator::new().build();
			let drain = slog_term::CompactFormat::new(decorator).build().fuse();
			let drain = slog_async::Async::new(drain).build().fuse();

			slog::Logger::root(drain, o!())
		});
		let logger = logger.new(o!("addr" => options.address.to_string()));

		// Try all addresses
		let addr: Box<Stream<Item = _, Error = _> + Send> =
			options.address.resolve(&logger);
		let private_key =
			match options.private_key.take().map(Ok).unwrap_or_else(|| {
				// Create new ECDH key
				crypto::EccKeyPrivP256::create()
			}) {
				Ok(key) => key,
				Err(e) => return Box::new(future::err(e.into())),
			};

		// Make options clonable
		let options = Arc::new(options);
		let logger2 = logger.clone();
		Box::new(
			addr.and_then(
				move |addr| -> Box<Future<Item = _, Error = _> + Send> {
					let (initserver_send, initserver_recv) = oneshot::channel();
					let (connection_send, connection_recv) = oneshot::channel();
					let ph: Option<PHBox> = options.handle_packets.as_ref().map(|h| (*h).clone());
					let packet_handler = SimplePacketHandler::new(
						logger.clone(),
						ph,
						initserver_send,
						connection_recv,
						#[cfg(feature = "audio")]
						options.audio_packet_handler.clone(),
					);
					let return_code_handler =
						packet_handler.return_codes.clone();
					let client = match client::new(
						options.local_address.unwrap_or_else(|| if addr.is_ipv4() {
							"0.0.0.0:0".parse().unwrap()
						} else {
							"[::]:0".parse().unwrap()
						}),
						private_key.clone(),
						packet_handler,
						logger.clone(),
					) {
						Ok(client) => client,
						Err(error) => return Box::new(future::err(error.into())),
					};

					{
						let mut c = client.lock();
						let c = &mut *c;
						// Logging
						if options.log_commands { log::add_command_logger(c); }
						if options.log_packets { log::add_packet_logger(c); }
						if options.log_udp_packets { log::add_udp_packet_logger(c); }
					}

					if let Some(prepare_client) = &options.prepare_client {
						prepare_client(&client);
					}

					let client = client.clone();
					let client2 = client.clone();
					let options = options.clone();

					// Create a connection
					debug!(logger, "Connecting"; "address" => %addr);
					let connect_fut = client::connect(
						Arc::downgrade(&client),
						&mut *client.lock(),
						addr,
					).from_err();

					let initserver_poll = initserver_recv
						.map_err(|e| {
							format_err!(
								"Error while waiting for initserver ({:?})",
								e
							).into()
						}).and_then(move |cmd| {
							let msg = InMessage::new(cmd).map_err(|(_, e)| e)?;
							if let InMessages::InitServer(_) = msg.msg() {
								Ok(msg)
							} else {
								Err(Error::ConnectionFailed(String::from(
									"Got no initserver",
								)))
							}
						});

					Box::new(
						connect_fut
				.and_then(move |con| -> Box<Future<Item=_, Error=_> + Send> {
					let logger = {
						let mutex = match con.upgrade().ok_or_else(||
							format_err!("Connection does not exist anymore")) {
							Ok(r) => r.mutex,
							Err(e) => return Box::new(future::err(e.into())),
						};
						let con = mutex.lock();
						con.1.logger.clone()
					};
					// TODO Add possibility to specify offset and level in ConnectOptions
					// Compute hash cash
					let mut time_reporter = slog_perf::TimeReporter::new_with_level(
						"Compute public key hash cash level", logger.clone(),
						slog::Level::Info);
					time_reporter.start("Compute public key hash cash level");
					let pub_k = {
						let c = client.lock();
						c.private_key.to_pub()
					};
					Box::new(future::poll_fn(move || {
						tokio_threadpool::blocking(|| {
							let res = (
								con.clone(),
								algs::hash_cash(&pub_k, 8).unwrap(),
								pub_k.to_ts().unwrap(),
								logger.clone(),
							);
							res
						})
					}).map(|r| { time_reporter.finish(); r })
					.map_err(|e| format_err!("Failed to start \
						blocking operation ({:?})", e).into()))
				})
				.and_then(move |(con, offset, omega, logger)| {
					info!(logger, "Computed hash cash level";
						"level" => algs::get_hash_cash_level(&omega, offset),
						"offset" => offset);

					// Create clientinit packet
					let version_string = options.version.get_version_string();
					let version_platform = options.version.get_platform();
					let version_sign = base64::encode(options.version.get_signature());
					let offset = offset.to_string();
					let packet = OutCommand::new::<_, _, String, String, _, _, std::iter::Empty<_>>(
						Direction::C2S,
						PacketType::Command,
						"clientinit",
						vec![
							("client_nickname", options.name.as_str()),
							("client_version", &version_string),
							("client_platform", &version_platform),
							("client_input_hardware", "1"),
							("client_output_hardware", "1"),
							("client_default_channel", ""),
							("client_default_channel_password", ""),
							("client_server_password", ""),
							("client_meta_data", ""),
							("client_version_sign", &version_sign),
							("client_nickname_phonetic", ""),
							("client_key_offset", &offset),
							("client_default_token", ""),
							("hwid", "923f136fb1e22ae6ce95e60255529c00,d13231b1bc33edfecfb9169cc7a63bcc"),
						].into_iter(),
						std::iter::empty(),
					);

					let sink = con.as_packet_sink();
					sink.send(packet).map(move |_| con)
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
						let con = mutex.lock();
						con.1.params.as_ref().ok_or_else(||
							format_err!("Connection params do not exist"))?
							.public_key.get_uid()?
					};

					// Create connection
					let data = data::Connection::new(Uid(uid), &initserver);
					let con = InnerConnection {
						connection: Arc::new(RwLock::new(data)),
						client_data: client2,
						client_connection: con,
						return_code_handler,
					};

					// Send connection to packet handler
					connection_send.send(con.connection.clone()).map_err(|_|
						format_err!("Failed to send connection to packet \
							handler"))?;

					Ok(Connection { inner: con })
				}),
					)
				},
			).then(move |r| -> Result<_> {
				if let Err(e) = &r {
					debug!(logger2, "Connecting failed, trying next address";
					"error" => ?e);
				}
				Ok(r.ok())
			}).filter_map(|r| r)
			.into_future()
			.map_err(|_| {
				Error::from(format_err!("Failed to connect to server"))
			}).and_then(|(r, _)| {
				r.ok_or_else(|| {
					format_err!("Failed to connect to server").into()
				})
			}),
		)
	}

	/// **This is part of the unstable interface.**
	///
	/// You can use it if you need access to lower level functions, but this
	/// interface may change on any version changes.
	pub fn get_packet_sink(
		&self,
	) -> impl Sink<SinkItem = OutPacket, SinkError = Error> {
		self.inner
			.client_connection
			.as_packet_sink()
			.sink_map_err(|e| e.into())
	}

	/// **This is part of the unstable interface.**
	///
	/// You can use it if you need access to lower level functions, but this
	/// interface may change on any version changes.
	pub fn get_udp_packet_sink(
		&self,
	) -> impl Sink<SinkItem = (PacketType, u16, bytes::Bytes), SinkError = Error>
	{
		self.inner
			.client_connection
			.as_udp_packet_sink()
			.sink_map_err(|e| e.into())
	}

	/// **This is part of the unstable interface.**
	///
	/// You can use it if you need access to lower level functions, but this
	/// interface may change on any version changes.
	///
	/// Adds a `return_code` to the command and returns if the corresponding
	/// answer is received. If an error occurs, the future will return an error.
	pub fn send_packet(
		&self,
		mut packet: OutPacket,
	) -> impl Future<Item = (), Error = Error>
	{
		// Store waiting in HashMap<usize (return code), oneshot::Sender>
		// The packet handler then sends a result to the sender if the answer is
		// received.

		let (code, recv) = self.inner.return_code_handler.get_return_code();
		// Add return code
		packet.data_mut().extend_from_slice("return_code=".as_bytes());
		packet.data_mut().extend_from_slice(code.to_string().as_bytes());

		// Send a message and wait until we get an answer for the return code
		self.get_packet_sink()
			.send(packet)
			.and_then(|_| {
				recv.map_err(|e| {
					format_err!("Too many return codes ({:?})", e).into()
				})
			})
			.and_then(|r| {
				if r == TsError::Ok {
					Ok(())
				} else {
					Err(r.into())
				}
			})
	}

	pub fn lock(&self) -> ConnectionLock {
		ConnectionLock::new(self.inner.connection.read())
	}

	pub fn to_mut<'a>(
		&self,
		con: &'a data::Connection,
	) -> data::ConnectionMut<'a>
	{
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
	pub fn disconnect<O: Into<Option<DisconnectOptions>>>(
		&self,
		options: O,
	) -> BoxFuture<()>
	{
		let options = options.into().unwrap_or_default();

		let mut args = Vec::new();
		if let Some(reason) = options.reason {
			args.push(("reasonid", (reason as u8).to_string()));
		}
		if let Some(msg) = options.message {
			args.push(("reasonmsg", msg));
		}

		let packet =
			OutCommand::new::<_, _, String, String, _, _, std::iter::Empty<_>>(
				Direction::C2S,
				PacketType::Command,
				"clientdisconnect",
				args.into_iter(),
				std::iter::empty(),
			);

		let addr = if let Some(con) = self.inner.client_connection.upgrade() {
			con.mutex.lock().1.address
		} else {
			return Box::new(future::ok(()));
		};
		let wait = self.inner.client_data.lock().wait_for_disconnect(addr);
		let inner = self.inner.clone();
		Box::new(
			self.inner
				.client_connection
				.as_packet_sink()
				.send(packet)
				.and_then(|_| wait)
				.from_err()
				// Make sure that the last reference lives long enough
				.map(move |_| drop(inner)),
		)
	}

	/// Set a function which will be called when this clients disconnects.
	///
	/// # Examples
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
	///         connection.add_on_disconnect(Box::new(|| {
	///             println!("Disconnected");
	///         }))
	///
	///         connection.disconnect(None)
	///     })
	///     .map_err(|_| ())
	/// );
	/// # }
	/// ```
	pub fn add_on_disconnect(&self, f: Box<FnOnce() + Send>) {
		self.inner.client_data.lock().connection_listeners.push(Box::new(
			DisconnectListener(Some(f))
		));
	}
}

#[cfg(feature = "audio")]
pub struct ConnectionPacketSinkCreator { con: Connection }
#[cfg(feature = "audio")]
impl ConnectionPacketSinkCreator {
	pub fn new(con: Connection) -> Self { Self { con } }
}

#[cfg(feature = "audio")]
impl tsproto_audio::audio_to_ts::PacketSinkCreator<Error> for ConnectionPacketSinkCreator {
	type S = Box<Sink<SinkItem=OutPacket, SinkError=Error> + Send>;
	fn get_sink(&self) -> Self::S { Box::new(self.con.get_packet_sink()) }
}

impl<CM: ConnectionManager> ConnectionListener<CM> for DisconnectListener {
	fn on_connection_removed(&mut self, _: &CM::Key, _: &mut ConnectionValue<CM::AssociatedData>) -> bool {
		self.0.take().unwrap()();
		false
	}
}

impl Drop for Connection {
	fn drop(&mut self) {
		if Arc::strong_count(&self.inner.connection) <= 2 {
			// The last 2 references are in the packet handler and this one
			// Disconnect
			let logger = self.inner.client_data.lock().logger.clone();
			tokio::spawn(self.disconnect(None).map_err(
				move |e| error!(logger, "Failed to disconnect"; "error" => ?e),
			));
		}
	}
}

impl<'a> ConnectionLock<'a> {
	fn new(guard: RwLockReadGuard<'a, data::Connection>) -> Self {
		Self { guard }
	}
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ServerAddress {
	SocketAddr(SocketAddr),
	Other(String),
}

impl From<SocketAddr> for ServerAddress {
	fn from(addr: SocketAddr) -> Self { ServerAddress::SocketAddr(addr) }
}

impl From<String> for ServerAddress {
	fn from(addr: String) -> Self { ServerAddress::Other(addr) }
}

impl<'a> From<&'a str> for ServerAddress {
	fn from(addr: &'a str) -> Self { ServerAddress::Other(addr.to_string()) }
}

impl ServerAddress {
	pub fn resolve(
		&self,
		logger: &Logger,
	) -> Box<Stream<Item = SocketAddr, Error = Error> + Send>
	{
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
	log_commands: bool,
	log_packets: bool,
	log_udp_packets: bool,
	#[cfg(feature = "audio")]
	audio_packet_handler: Option<AudioPacketHandler>,
	handle_packets: Option<PHBox>,
	prepare_client: Option<
		Box<Fn(&client::ClientDataM<SimplePacketHandler>) + Send + Sync>,
	>,
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
			log_commands: false,
			log_packets: false,
			log_udp_packets: false,
			#[cfg(feature = "audio")]
			audio_packet_handler: None,
			handle_packets: None,
			prepare_client: None,
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
	pub fn private_key(mut self, private_key: crypto::EccKeyPrivP256) -> Self {
		self.private_key = Some(private_key);
		self
	}

	/// Takes the private key as a string. The exact format is determined
	/// automatically.
	///
	/// # Default
	/// A new identity is generated when connecting.
	///
	/// # Error
	/// An error is returned if the string cannot be decoded.
	#[inline]
	pub fn private_key_str(mut self, private_key: &str) -> Result<Self> {
		self.private_key = Some(crypto::EccKeyPrivP256::import_str(private_key)?);
		Ok(self)
	}

	/// Takes the private key as a byte slice. The exact format is determined
	/// automatically.
	///
	/// # Default
	/// A new identity is generated when connecting.
	///
	/// # Error
	/// An error is returned if the byte slice cannot be decoded.
	#[inline]
	pub fn private_key_bytes(mut self, private_key: &[u8]) -> Result<Self> {
		self.private_key = Some(crypto::EccKeyPrivP256::import(private_key)?);
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

	/// If the content of all commands should be written to the logger.
	///
	/// # Default
	/// `false`
	#[inline]
	pub fn log_commands(mut self, log_commands: bool) -> Self {
		self.log_commands = log_commands;
		self
	}

	/// If the content of all packets in high-level form should be written to
	/// the logger.
	///
	/// # Default
	/// `false`
	#[inline]
	pub fn log_packets(mut self, log_packets: bool) -> Self {
		self.log_packets = log_packets;
		self
	}

	/// If the content of all udp packets in byte-array form should be written
	/// to the logger.
	///
	/// # Default
	/// `false`
	#[inline]
	pub fn log_udp_packets(mut self, log_udp_packets: bool) -> Self {
		self.log_udp_packets = log_udp_packets;
		self
	}

	/// If the client should.
	///
	/// # Default
	/// `false`
	#[cfg(feature = "audio")]
	#[inline]
	pub fn audio_packet_handler(mut self,
		audio_packet_handler: AudioPacketHandler) -> Self {
		self.audio_packet_handler = Some(audio_packet_handler);
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
	pub fn handle_packets(mut self, handle_packets: PHBox) -> Self {
		self.handle_packets = Some(handle_packets);
		self
	}

	/// **This is part of the unstable interface.**
	///
	/// You can use it if you need access to lower level functions, but this
	/// interface may change on any version changes.
	///
	/// This can be used to access the underlying client before it is used to
	/// connect to a server.
	///
	/// The given function is called with the client as an argument. This may
	/// happen more than one time, if different ip addresses of the server are
	/// tried.
	///
	/// # Default
	/// The client is setup the default way.
	#[inline]
	pub fn prepare_client(
		mut self,
		prepare_client: Box<
			Fn(&client::ClientDataM<SimplePacketHandler>) + Send + Sync,
		>,
	) -> Self
	{
		self.prepare_client = Some(prepare_client);
		self
	}
}

impl fmt::Debug for ConnectOptions {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		// Error if attributes are added
		let ConnectOptions {
			address,
			local_address,
			private_key,
			name,
			version,
			logger,
			log_commands,
			log_packets,
			log_udp_packets,
			#[cfg(feature = "audio")]
			audio_packet_handler,
			handle_packets: _,
			prepare_client: _,
		} = self;
		write!(
			f,
			"ConnectOptions {{ address: {:?}, local_address: {:?}, \
			 private_key: {:?}, name: {}, version: {}, logger: {:?}, \
			 log_commands: {}, log_packets: {}, log_udp_packets: {},",
			address,
			local_address,
			private_key,
			name,
			version,
			logger,
			log_commands,
			log_packets,
			log_udp_packets,
		)?;
		#[cfg(feature = "audio")]
		write!(f, ", audio_packet_handler: {:?}", audio_packet_handler)?;
		write!(f, " }}")?;
		Ok(())
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
	pub fn new() -> Self { Self::default() }

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
