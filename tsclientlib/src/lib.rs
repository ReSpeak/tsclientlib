//! tsclientlib is a library which makes it simple to create TeamSpeak clients
//! and bots.
//!
//! If you want a full client application, you might want to have a look at
//! [Qint].
//!
//! The base class of this library is the [`Connection`]. One instance of this
//! struct manages a single connection to a server.
//!
//! [`Connection`]: struct.Connection.html
//! [Qint]: https://github.com/ReSpeak/Qint
// Needed for futures on windows.
#![recursion_limit = "128"]

use std::collections::HashMap;
use std::fmt;
use std::marker::PhantomData;
use std::net::SocketAddr;
use std::ops::Deref;
use std::sync::Arc;

use derive_more::From;
use failure::{format_err, Fail, ResultExt};
use futures::sync::{mpsc, oneshot};
use futures::{future, stream, Future, Sink, Stream};
use parking_lot::{Mutex, RwLock, RwLockReadGuard};
use slog::{debug, info, o, warn, Drain, Logger};
use tokio::net::TcpStream;
use tsproto::connectionmanager::ConnectionManager;
use tsproto::connectionmanager::Resender;
use tsproto::handler_data::{ConnectionListener, ConnectionValue};
use tsproto_packets::packets::{
	Direction, InAudio, InCommand, OutCommand, OutPacket, PacketType,
};
use tsproto::{client, log};
use ts_bookkeeping::messages::c2s;
use ts_bookkeeping::messages::s2c::{InMessage, InMessages};

use crate::packet_handler::{ReturnCodeHandler, SimplePacketHandler};
use crate::filetransfer::{FileTransferIdHandle, FileTransferHandler,
	FileTransferStatus};

mod facades;
mod filetransfer;
mod packet_handler;
pub mod resolver;

// The build environment of tsclientlib.
git_testament::git_testament!(TESTAMENT);

#[cfg(test)]
mod tests;

// Reexports
pub use ts_bookkeeping::*;
pub use tsproto::connection::Identity;

type BoxFuture<T> = Box<dyn Future<Item = T, Error = Error> + Send>;
type Result<T> = std::result::Result<T, Error>;
pub type EventListener =
	Box<dyn Fn(&Event) + Send + Sync>;

#[derive(Fail, Debug, From)]
pub enum Error {
	#[fail(display = "{}", _0)]
	Base64(#[cause] base64::DecodeError),
	#[fail(display = "{}", _0)]
	Bookkeeping(#[cause] ts_bookkeeping::Error),
	#[fail(display = "{}", _0)]
	Canceled(#[cause] futures::Canceled),
	#[fail(display = "{}", _0)]
	DnsProto(#[cause] trust_dns_proto::error::ProtoError),
	#[fail(display = "{}", _0)]
	Io(#[cause] std::io::Error),
	#[fail(display = "{}", _0)]
	ParseMessage(#[cause] ts_bookkeeping::messages::ParseError),
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
	#[fail(display = "Not an error – non exhaustive enum")]
	__NonExhaustive,
}

impl From<failure::Error> for Error {
	fn from(e: failure::Error) -> Self {
		let r: std::result::Result<(), _> = Err(e);
		Error::Other(r.compat().unwrap_err())
	}
}

pub type PHBox = Box<dyn PacketHandler + Send + Sync>;
pub trait PacketHandler {
	fn new_connection(
		&mut self,
		command_stream: Box<
			dyn Stream<Item = InCommand, Error = tsproto::Error> + Send,
		>,
		audio_stream: Box<
			dyn Stream<Item = InAudio, Error = tsproto::Error> + Send,
		>,
	);
	/// Clone into a box.
	fn clone(&self) -> PHBox;
}

#[derive(Clone)]
pub enum Event<'a> {
	ConEvents(&'a ConnectionLock<'a>, &'a [events::Event]),
	// Events that occur before the connection is created
	/// The identity and the needed level.
	IdentityLevelIncreasing(&'a Identity, u8),
	/// The identity and the reached level.
	///
	/// This event may occur without an `IdentityLevelIncreasing` event before
	/// if a new identity is created because no identity was supplied.
	IdentityLevelIncreased(&'a Identity),
}

pub struct ConnectionLock<'a> {
	connection: Connection,
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
	file_transfer_handler: Arc<FileTransferHandler>,
	event_listeners: Arc<RwLock<HashMap<String, EventListener>>>,
}

#[derive(Clone)]
pub struct Connection {
	inner: InnerConnection,
}

struct DisconnectListener(Option<Box<dyn Fn() + Send>>);

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
	#[must_use = "futures do nothing unless polled"]
	pub fn new(mut options: ConnectOptions) -> BoxFuture<Connection> {
		let logger = options.logger.take().unwrap_or_else(|| {
			let decorator = slog_term::TermDecorator::new().build();
			let drain = slog_term::CompactFormat::new(decorator).build().fuse();
			let drain = slog_async::Async::new(drain).build().fuse();

			slog::Logger::root(drain, o!())
		});

		#[cfg(debug_assertions)]
		let profile = "Debug";
		#[cfg(not(debug_assertions))]
		let profile = "Release";

		info!(logger, "TsClientlib";
			"version" => git_testament::render_testament!(TESTAMENT),
			"profile" => profile,
			"tsproto-version" => git_testament::render_testament!(tsproto::get_testament()),
		);

		let logger = logger.new(o!("addr" => options.address.to_string()));

		// Try all addresses
		let addr: Box<dyn Stream<Item = _, Error = _> + Send> =
			options.address.resolve(&logger);
		options.identity =
			match options.identity.take().map(Ok).unwrap_or_else(|| {
				// Create new ECDH key
				let id = Identity::create();
				if let Ok(id) = &id {
					// Send event
					let e = Event::IdentityLevelIncreased(id);
					for (_, l) in &options.event_listeners {
						l(&e);
					}
				}
				id
			}) {
				Ok(id) => Some(id),
				Err(e) => return Box::new(future::err(e.into())),
			};

		// Make options clonable
		let options = Arc::new(Mutex::new(options));
		let logger2 = logger.clone();
		Box::new(
			addr.and_then(move |addr| Self::connect_to(logger.clone(), options.clone(), addr))
			.then(move |r| -> Result<_> {
				if let Err(e) = &r {
					debug!(logger2, "Connecting failed, trying next address";
					"error" => ?e);
				}
				Ok(r.ok())
			})
			.filter_map(|r| r)
			.into_future()
			.map_err(|_| {
				Error::from(format_err!("Failed to connect to server"))
			})
			.and_then(|(r, _)| {
				r.ok_or_else(|| {
					format_err!("Failed to connect to server").into()
				})
			}),
		)
	}

	fn connect_to(logger: Logger, options: Arc<Mutex<ConnectOptions>>, addr: SocketAddr) -> BoxFuture<Connection> {
		let (initserver_send, initserver_recv) = oneshot::channel();
		let (connection_send, connection_recv) = oneshot::channel();
		let opts = options.lock();
		let ph: Option<PHBox> =
			opts.handle_packets.as_ref().map(|h| (*h).clone());
		let packet_handler = SimplePacketHandler::new(
			logger.clone(),
			ph,
			initserver_send,
			connection_recv,
		);
		let counter = opts.identity.as_ref().unwrap().counter().to_string();
		let return_code_handler = packet_handler.return_codes.clone();
		let file_transfer_handler = packet_handler.ft_ids.clone();
		let client = match client::new(
			opts.local_address.unwrap_or_else(|| {
				if addr.is_ipv4() {
					"0.0.0.0:0".parse().unwrap()
				} else {
					"[::]:0".parse().unwrap()
				}
			}),
			opts.identity.as_ref().unwrap().key().clone(),
			packet_handler,
			logger.clone(),
		) {
			Ok(client) => client,
			Err(error) => {
				return Box::new(future::err(error.into()))
			}
		};

		{
			let mut c = client.lock();
			let c = &mut *c;
			// Logging
			if opts.log_commands {
				log::add_command_logger(c);
			}
			if opts.log_packets {
				log::add_packet_logger(c);
			}
			if opts.log_udp_packets {
				log::add_udp_packet_logger(c);
			}
		}

		if let Some(prepare_client) = &opts.prepare_client {
			prepare_client(&client);
		}
		drop(opts);

		let client2 = client.clone();

		// Create a connection
		debug!(logger, "Connecting"; "address" => %addr);
		let connect_fut = client::connect(
			Arc::downgrade(&client),
			&mut *client.lock(),
			addr,
		)
		.from_err();

		let options2 = options.clone();
		let options3 = options.clone();
		Box::new(
			connect_fut.and_then(move |con| {
				// Create clientinit packet
				let opts = options.lock();
				let version_string = opts.version.get_version_string();
				let version_platform = opts.version.get_platform();
				let version_sign = base64::encode(opts.version.get_signature());

				let mut args = vec![
					("client_nickname", opts.name.as_str()),
					("client_version", &version_string),
					("client_platform", &version_platform),
					("client_input_hardware", "1"),
					("client_output_hardware", "1"),
					("client_default_channel_password", ""),
					("client_server_password", ""),
					("client_meta_data", ""),
					("client_version_sign", &version_sign),
					("client_nickname_phonetic", ""),
					("client_key_offset", &counter),
					("client_default_token", ""),
					("hwid", "923f136fb1e22ae6ce95e60255529c00,d13231b1bc33edfecfb9169cc7a63bcc"),
				];

				if let Some(channel) = &opts.channel {
					args.push(("client_default_channel", channel));
				}

				let packet = OutCommand::new::<_, _, String, String, _, _, std::iter::Empty<_>>(
					Direction::C2S,
					PacketType::Command,
					"clientinit",
					args.into_iter(),
					std::iter::empty(),
				);

				let sink = con.as_packet_sink();
				sink.send(packet).map(move |_| con)
			})
			.from_err()
			// Wait until we sent the clientinit packet and afterwards received
			// the initserver packet.
				.and_then(move |con| {
					initserver_recv.map(|r| (con, r))
						.map_err(|e| {
							format_err!(
								"Error while waiting for initserver ({:?})",
								e
							)
								.into()
						})
				})
			.and_then(move |(con, cmd)| {
				Self::handle_initserver(options3, con, cmd)
					.and_then(move |r| -> BoxFuture<Self> {
						if let Some((con, initserver)) = r {
							// Get uid of server
							let uid = {
								let con = if let Some(r) = con.upgrade() {
									r
								} else {
									return Box::new(future::err(
										format_err!("Connection does not exist anymore").into()));
								};
								let mutex = con.mutex;
								let con = mutex.lock();
								let params = if let Some(r) = &con.1.params {
									r
								} else {
									return Box::new(future::err(
										format_err!("Connection params do not exist").into()));
								};

								match params.public_key.get_uid() {
									Ok(r) => r,
									Err(e) => return Box::new(future::err(e.into())),
								}
							};

							// Create connection
							let data = data::Connection::new(Uid(uid), &initserver);
							let con = InnerConnection {
								connection: Arc::new(RwLock::new(data)),
								client_data: client2,
								client_connection: con,
								return_code_handler,
								file_transfer_handler,
								event_listeners: Arc::new(RwLock::new(HashMap::new())),
							};

							// Send connection to packet handler
							let con = Connection { inner: con };
							let mut opts = options2.lock();
							if let Err(_) = connection_send.send(con.clone()) {
								return Box::new(future::err(
									format_err!("Failed to send connection to \
									packet handler").into()));
							}

							let events = vec![events::Event::PropertyAdded {
								id: events::PropertyId::Server,
								invoker: None,
							}];
							{
								let con_lock = con.lock();
								let ev = Event::ConEvents(&con_lock, &events);
								for (k, l) in opts.event_listeners.drain(..) {
									l(&ev);
									con.add_event_listener(k, l);
								}
							}

							Box::new(future::ok(con))
						} else {
							// Try connecting again
							Self::connect_to(logger, options2, addr)
						}
					})
			})
		)
	}

	/// If this returns `None`, the level was increased and we should try
	/// connecting again.
	fn handle_initserver(
		options: Arc<Mutex<ConnectOptions>>,
		con: client::ClientConVal,
		cmd: InCommand,
	) -> BoxFuture<Option<(client::ClientConVal, InMessage)>>
	{
		let msg = match InMessage::new(cmd).map_err(|(_, e)| e) {
			Ok(r) => r,
			Err(e) => {
				return Box::new(future::err(e.into()));
			}
		};
		if let InMessages::InitServer(_) = msg.msg() {
			Box::new(future::ok(Some((con, msg))))
		} else if let InMessages::CommandError(e) = msg.msg() {
			let e = e.iter().next().unwrap();
			if e.id == ts_bookkeeping::TsError::ClientCouldNotValidateIdentity {
				if let Some(needed) = e.extra_message.and_then(|m| m.parse::<u8>().ok()) {
					if needed > 20 {
						return Box::new(future::err(Error::ConnectionFailed(
							format!("The server needs an \
								identity of level {}, please \
								increase your identity level", needed)
						)));
					}

					{
						let opts = options.lock();
						match opts.identity.as_ref().unwrap().level() {
							Ok(level) => {
								if level >= needed {
									return Box::new(future::err(Error::ConnectionFailed(
										format!("The server requested an \
											identity of level {}, but we already \
											have level {}", needed, level)
									)));
								}
							}
							Err(e) => return Box::new(future::err(e.into())),
						}

						let e = Event::IdentityLevelIncreasing(opts.identity.as_ref().unwrap(), needed);
						for (_, l) in &opts.event_listeners {
							l(&e);
						}
					}

					// Increase identity level
					let options2 = options.clone();
					let (send, recv) = oneshot::channel();
					// TODO Performance log
					std::thread::spawn(move || {
						let mut opts = options.lock();
						let _ = send.send(opts.identity.as_mut().unwrap()
							.upgrade_level(needed).map_err(|e| e.into()));
					});
					return Box::new(recv.from_err().and_then(|r| r).map(move |()| {
						let opts = options2.lock();
						let e = Event::IdentityLevelIncreased(&opts.identity.as_ref().unwrap());
						for (_, l) in &opts.event_listeners {
							l(&e);
						}
						// Try to connect again
						None
					}));
				}
			}
			Box::new(future::err(Error::ConnectionFailed(
				format!("Got no initserver but {:?}", msg)
			)))
		} else {
			Box::new(future::err(Error::ConnectionFailed(
				format!("Got no initserver but {:?}", msg)
			)))
		}
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
	) -> impl Sink<SinkItem = (PacketType, u32, u16, bytes::Bytes), SinkError = Error>
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
	pub fn get_tsproto_connection(&self) -> client::ClientConVal {
		self.inner.client_connection.clone()
	}

	/// **This is part of the unstable interface.**
	///
	/// You can use it if you need access to lower level functions, but this
	/// interface may change on any version changes.
	///
	/// Returns the public key of the server.
	pub fn get_server_key(&self) -> Result<tsproto::crypto::EccKeyPubP256> {
		let con = if let Some(con) = self.inner.client_connection.upgrade() {
			con
		} else {
			return Err(format_err!("Connection is gone").into());
		};
		let con = con.mutex.lock();

		if let Some(params) = &con.1.params {
			Ok(params.public_key.clone())
		} else {
			Err(format_err!("Connection should be connected but has no params")
				.into())
		}
	}

	/// **This is part of the unstable interface.**
	///
	/// You can use it if you need access to lower level functions, but this
	/// interface may change on any version changes.
	///
	/// Adds a `return_code` to the command and returns if the corresponding
	/// answer is received. If an error occurs, the future will return an error.
	#[must_use = "futures do nothing unless polled"]
	pub fn send_packet(
		&self,
		mut packet: OutPacket,
	) -> impl Future<Item = (), Error = Error> + Send + 'static
	{
		// Store waiting in HashMap<usize (return code), oneshot::Sender>
		// The packet handler then sends a result to the sender if the answer is
		// received.

		let (code, recv) = self.inner.return_code_handler.get_return_code();
		// Add return code
		packet
			.data_mut()
			.extend_from_slice(" return_code=".as_bytes());
		packet
			.data_mut()
			.extend_from_slice(code.to_string().as_bytes());

		// Send a message and wait until we get an answer for the return code
		self.get_packet_sink()
			.send(packet)
			.and_then(|_| recv.from_err())
			.and_then(|r| {
				if r == TsError::Ok {
					Ok(())
				} else {
					Err(r.into())
				}
			})
	}

	pub fn lock(&self) -> ConnectionLock {
		ConnectionLock::new(self.clone(), self.inner.connection.read())
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
	#[must_use = "futures do nothing unless polled"]
	pub fn disconnect<O: Into<Option<DisconnectOptions>>>(
		&self,
		options: O,
	) -> BoxFuture<()>
	{
		let packet = self.inner.connection.read().disconnect(options);

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
	///         }));
	///
	///         connection.disconnect(None)
	///     })
	///     .map_err(|_| ())
	/// );
	/// # }
	/// ```
	pub fn add_on_disconnect(&self, f: Box<dyn Fn() + Send>) {
		self.inner
			.client_data
			.lock()
			.connection_listeners
			.push(Box::new(DisconnectListener(Some(f))));
	}

	/// Set a function which will be called on events.
	///
	/// An event is generated e.g. when a property of a client or channel
	/// changes.
	///
	/// The `key` can be freely chosen, it is needed to remove the the listener
	/// again. It should be unique as any old event listener with this key will
	/// be removed and returned. Internally all listeners are stored in a
	/// `HashMap`.
	pub fn add_event_listener(
		&self,
		key: String,
		f: EventListener,
	) -> Option<EventListener>
	{
		self.inner.event_listeners.write().insert(key, f)
	}

	/// Remove an event listener which was registered with the specified `key`.
	///
	/// The removed event listener is returned if the key was found in the
	/// listeners.
	pub fn remove_event_listener(&self, key: &str) -> Option<EventListener> {
		self.inner.event_listeners.write().remove(key)
	}

	/// Return the size of the file and a tcp stream of the requested file.
	pub fn download_file(&self, channel_id: ChannelId, path: &str,
		channel_password: Option<&str>, seek_position: Option<u64>) -> BoxFuture<(u64, TcpStream)> {
		let (code_handle, recv) = FileTransferHandler::get_file_transfer_id(self.inner.file_transfer_handler.clone());

		let packet = c2s::OutFtInitDownloadMessage::new(
			vec![c2s::FtInitDownloadPart {
				client_file_transfer_id: code_handle.id,
				name: path,
				channel_id,
				channel_password: channel_password.unwrap_or(""),
				seek_position: seek_position.unwrap_or_default(),
				protocol: 1,
				phantom: PhantomData,
			}]
			.into_iter(),
		);

		self.get_file_stream(packet, code_handle, recv)
	}

	/// **This is part of the unstable interface.**
	///
	/// Return the size of the file, the port, ip and the token.
	///
	/// TODO This is temporary code until I get my runtime fixed.
	#[doc(hidden)]
	pub fn download_file_token(&self, channel_id: ChannelId, path: &str,
		channel_password: Option<&str>, seek_position: Option<u64>) -> BoxFuture<(TokenWrapper, u64, SocketAddr)> {
		let (code_handle, recv) = FileTransferHandler::get_file_transfer_id(self.inner.file_transfer_handler.clone());

		let packet = c2s::OutFtInitDownloadMessage::new(
			vec![c2s::FtInitDownloadPart {
				client_file_transfer_id: code_handle.id,
				name: path,
				channel_id,
				channel_password: channel_password.unwrap_or(""),
				seek_position: seek_position.unwrap_or_default(),
				protocol: 1,
				phantom: PhantomData,
			}]
			.into_iter(),
		);

		self.get_file_stream_token(packet, code_handle, recv)
	}

	/// Return the size of the part which is already uploaded (when resume is
	/// specified) and a tcp stream where the requested file should be uploaded.
	pub fn upload_file(&self, channel_id: ChannelId, path: &str,
		channel_password: Option<&str>, size: u64, overwrite: bool, resume: bool
	) -> BoxFuture<(u64, TcpStream)> {
		let (code_handle, recv) = FileTransferHandler::get_file_transfer_id(self.inner.file_transfer_handler.clone());

		let packet = c2s::OutFtInitUploadMessage::new(
			vec![c2s::FtInitUploadPart {
				client_file_transfer_id: code_handle.id,
				name: path,
				channel_id,
				channel_password: channel_password.unwrap_or(""),
				overwrite,
				resume,
				size,
				protocol: 1,
				phantom: PhantomData,
			}]
			.into_iter(),
		);

		self.get_file_stream(packet, code_handle, recv)
	}

	fn get_file_stream(&self, packet: OutPacket, code_handle: FileTransferIdHandle,
		recv: mpsc::UnboundedReceiver<FileTransferStatus>
	) -> BoxFuture<(u64, TcpStream)> {
		let default_ip;
		if let Some(con) = self.inner.client_connection.upgrade() {
			default_ip = con.mutex.lock().1.address.ip();
		} else {
			return Box::new(future::err(format_err!("Connection is gone").into()));
		}

		// Send a message and wait until we get an answer for the return code
		Box::new(self.get_packet_sink()
			.send(packet)
			.and_then(|_| recv.into_future().map_err(|(e, _)| e.into()))
			.and_then(move |(status, _recv)| {
				match status {
					Some(FileTransferStatus::Start {
						key,
						port,
						size,
						ip,
					}) => {
						let ip = ip.unwrap_or(default_ip);
						Ok((key, size, SocketAddr::new(ip, port)))
					}
					Some(FileTransferStatus::Status { status }) => {
						Err(status.into())
					}
					None => Err(format_err!("Connection canceled").into()),
				}
			})
			.and_then(|(key, size, addr)| TcpStream::connect(&addr)
				.and_then(move |s| {
					tokio::io::write_all(s, key)
				})
				.and_then(move |(s, _)| {
					tokio::io::flush(s)
				})
				.from_err()
				.map(move |s| {
					// TODO Select with recv
					// Drop file transfer code
					drop(code_handle);
					(size, s)
				})
			)
		)
	}

	// TODO This is temporary code until I get my runtime fixed.
	fn get_file_stream_token(&self, packet: OutPacket, code_handle: FileTransferIdHandle,
		recv: mpsc::UnboundedReceiver<FileTransferStatus>
	) -> BoxFuture<(TokenWrapper, u64, SocketAddr)> {
		let default_ip;
		if let Some(con) = self.inner.client_connection.upgrade() {
			default_ip = con.mutex.lock().1.address.ip();
		} else {
			return Box::new(future::err(format_err!("Connection is gone").into()));
		}

		// Send a message and wait until we get an answer for the return code
		Box::new(self.get_packet_sink()
			.send(packet)
			.and_then(|_| recv.into_future().map_err(|(e, _)| e.into()))
			.and_then(move |(status, _recv)| {
				match status {
					Some(FileTransferStatus::Start {
						key,
						port,
						size,
						ip,
					}) => {
						let ip = ip.unwrap_or(default_ip);
						let token = TokenWrapper {
							token: key,
							_code_handle: code_handle,
						};
						Ok((token, size, SocketAddr::new(ip, port)))
					}
					Some(FileTransferStatus::Status { status }) => {
						Err(status.into())
					}
					None => Err(format_err!("Connection canceled").into()),
				}
			})
		)
	}
}

/// TODO Temporary struct
#[doc(hidden)]
pub struct TokenWrapper {
	pub token: String,
	_code_handle: FileTransferIdHandle,
}

impl<CM: ConnectionManager> ConnectionListener<CM> for DisconnectListener {
	fn on_connection_removed(
		&mut self,
		_: &CM::Key,
		_: &mut ConnectionValue<CM::AssociatedData>,
	) -> bool
	{
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
			// Check that we are not yet disconnecting
			if let Some(con) = self.inner.client_connection.upgrade() {
				if con.mutex.lock().1.resender.is_disconnecting() {
					return;
				}
			} else {
				return;
			}
			tokio::spawn(self.disconnect(None).map_err(move |e| {
				warn!(logger, "Failed to disconnect from destructor";
					"error" => ?e)
			}));
		}
	}
}

impl<'a> ConnectionLock<'a> {
	fn new(
		connection: Connection,
		guard: RwLockReadGuard<'a, data::Connection>,
	) -> Self
	{
		Self { connection, guard }
	}

	/// Get a connection where you can change properties.
	///
	/// Changing properties will send a packet to the server, it will not
	/// immediately change the value.
	pub fn to_mut(&'a self) -> facades::ConnectionMut<'a> {
		facades::ConnectionMut {
			connection: self.connection.clone(),
			inner: &*self.guard,
		}
	}

	/// This returns a clone of the locked connection.
	///
	/// To access the connection data, use the dereferenced object of this
	/// `ConnectionLock`.
	pub fn get_locked(&self) -> Connection {
		self.connection.clone()
	}
}

trait ServerAddressExt {
	fn resolve(&self, logger: &Logger) -> Box<dyn Stream<Item = SocketAddr, Error = Error> + Send>;
}

impl ServerAddressExt for ServerAddress {
	fn resolve(
		&self,
		logger: &Logger,
	) -> Box<dyn Stream<Item = SocketAddr, Error = Error> + Send>
	{
		match self {
			ServerAddress::SocketAddr(a) => Box::new(stream::once(Ok(*a))),
			ServerAddress::Other(s) => Box::new(resolver::resolve(logger, s)),
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
	identity: Option<Identity>,
	name: String,
	version: Version,
	channel: Option<String>,
	logger: Option<Logger>,
	log_commands: bool,
	log_packets: bool,
	log_udp_packets: bool,
	event_listeners: Vec<(String, EventListener)>,
	handle_packets: Option<PHBox>,
	prepare_client: Option<
		Box<dyn Fn(&client::ClientDataM<SimplePacketHandler>) + Send + Sync>,
	>,
}

impl ConnectOptions {
	/// Start creating the configuration of a new connection.
	///
	/// # Arguments
	/// The address of the server has to be supplied. The address can be a
	/// [`SocketAddr`], a string or directly a [`ServerAddress`]. A string
	/// will automatically be resolved from all formats supported by TeamSpeak.
	/// For details, see [`resolver::resolve`].
	///
	/// [`SocketAddr`]: ../../std/net/enum.SocketAddr.html
	/// [`ServerAddress`]: enum.ServerAddress.html
	/// [`resolver::resolve`]: resolver/method.resolve.html
	#[inline]
	pub fn new<A: Into<ServerAddress>>(address: A) -> Self {
		Self {
			address: address.into(),
			local_address: None,
			identity: None,
			name: String::from("TeamSpeakUser"),
			version: Version::Windows_3_X_X__1,
			channel: None,
			logger: None,
			log_commands: false,
			log_packets: false,
			log_udp_packets: false,
			event_listeners: Vec::new(),
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

	/// Set the identity of the user.
	///
	/// # Default
	/// A new identity is generated when connecting.
	#[inline]
	pub fn identity(mut self, identity: Identity) -> Self {
		self.identity = Some(identity);
		self
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

	/// Connect to a specific channel.
	///
	/// # Example
	/// ```
	/// # use tsclientlib::ConnectOptions;
	/// let opts = ConnectOptions::new("localhost").channel("Default Channel".to_string());
	/// ```
	///
	/// Connecting to a channel further down in the hierarchy.
	/// ```
	/// # use tsclientlib::ConnectOptions;
	/// let opts = ConnectOptions::new("localhost")
	///		.channel("Default Channel/Nested".to_string());
	/// ```
	#[inline]
	pub fn channel(mut self, channel: String) -> Self {
		self.channel = Some(channel);
		self
	}

	/// Connect to a specific channel.
	///
	/// Setting the channel id is equal to connecting to the channel `/<id>`.
	///
	/// # Example
	/// ```
	/// # use tsclientlib::{ChannelId, ConnectOptions};
	/// let opts = ConnectOptions::new("localhost").channel_id(ChannelId(2));
	/// ```
	#[inline]
	pub fn channel_id(mut self, channel: ChannelId) -> Self {
		self.channel = Some(format!("/{}", channel.0));
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

	/// Set a custom logger for the connection.
	///
	/// # Default
	/// A new logger is created.
	#[inline]
	pub fn logger(mut self, logger: Logger) -> Self {
		self.logger = Some(logger);
		self
	}

	/// Set a function which will be called on events.
	///
	/// An event is generated e.g. when a property of a client or channel
	/// changes.
	///
	/// The `key` can be freely chosen, it is needed to remove the the listener
	/// again. It should be unique as any old event listener with this key will
	/// be removed and returned. Internally all listeners are stored in a
	/// `HashMap`.
	#[inline]
	pub fn add_event_listener(mut self, key: String, event_listener: EventListener) -> Self {
		self.event_listeners.push((key, event_listener));
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
			dyn Fn(&client::ClientDataM<SimplePacketHandler>) + Send + Sync,
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
			identity,
			name,
			version,
			channel,
			logger,
			log_commands,
			log_packets,
			log_udp_packets,
			event_listeners: _,
			handle_packets: _,
			prepare_client: _,
		} = self;
		write!(
			f,
			"ConnectOptions {{ address: {:?}, local_address: {:?}, \
			 identity: {:?}, name: {}, version: {}, channel: {:?}, \
			 logger: {:?}, log_commands: {}, log_packets: {}, \
			 log_udp_packets: {},",
			address,
			local_address,
			identity,
			name,
			version,
			channel,
			logger,
			log_commands,
			log_packets,
			log_udp_packets,
		)?;
		write!(f, " }}")?;
		Ok(())
	}
}
