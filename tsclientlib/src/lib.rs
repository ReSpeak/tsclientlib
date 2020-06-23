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

use std::borrow::Cow;
use std::collections::VecDeque;
use std::iter;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};

use futures::prelude::*;
use slog::{debug, info, o, warn, Drain, Logger};
use thiserror::Error;
use tokio::io::AsyncWriteExt as _;
use tokio::net::{TcpStream, UdpSocket};
use tokio::sync::oneshot;
use tokio::time;
use ts_bookkeeping::messages::c2s;
use ts_bookkeeping::messages::s2c::InMessage;
use tsproto::client;
use tsproto::connection::StreamItem as ProtoStreamItem;
use tsproto::resend::ResenderState;
#[cfg(feature = "audio")]
use tsproto_packets::packets::InAudioBuf;
use tsproto_packets::packets::{InCommandBuf, OutCommand};

#[cfg(feature = "audio")]
pub mod audio;
pub mod facades;
pub mod resolver;
pub mod sync;

// The build environment of tsclientlib.
git_testament::git_testament!(TESTAMENT);

#[cfg(test)]
mod tests;

// Reexports
pub use ts_bookkeeping::*;
pub use tsproto::Identity;

/// Wait this time for initserver, in seconds.
const INITSERVER_TIMEOUT: u64 = 5;

type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct MessageHandle(pub u16);
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct FileTransferHandle(pub u16);

// TODO Sort
#[derive(Debug, Error)]
pub enum Error {
	/// A command return an error.
	#[error(transparent)]
	CommandError(#[from] tsproto_types::errors::Error),
	#[error("File transfer failed: {0}")]
	FileTransferIo(#[source] std::io::Error),
	#[error("The server needs an identity of level {0}, please increase your identity level")]
	IdentityLevel(u8),
	#[error(
		"The server requested an identity of level {needed}, but we already have level {have}"
	)]
	IdentityLevelCorrupted { needed: u8, have: u8 },
	#[error("Failed to increase identity level: {0}")]
	IdentityLevelIncreaseFailed(#[source] tsproto::Error),
	#[error("Failed to increase identity level: Thread died")]
	IdentityLevelIncreaseFailedThread,
	#[error("Failed to create identity: {0}")]
	IdentityCreate(#[source] tsproto::Error),
	/// The connection is currently not connected to a server but is in the process of connecting.
	#[error("Currently not connected")]
	NotConnected,
	#[error("Failed to connect: {0}")]
	Connect(#[source] tsproto::client::Error),
	#[error("Server refused connection: {0}")]
	ConnectTs(#[source] tsproto_types::errors::Error),
	#[error("Failed to send clientinit: {0}")]
	SendClientinit(#[source] tsproto::client::Error),
	#[error("Failed to send packet: {0}")]
	SendPacket(#[source] tsproto::client::Error),
	#[error("Timeout while waiting for initserver")]
	InitserverTimeout,
	#[error("Failed to receive initserver: {0}")]
	InitserverWait(#[source] tsproto::client::Error),
	#[error("Failed to parse initserver: {0}")]
	InitserverParse(#[source] ts_bookkeeping::messages::ParseError),
	#[error("We should be connected but the connection params do not exist")]
	InitserverParamsMissing,
	/// The connection was destroyed.
	#[error("Connection does not exist anymore")]
	ConnectionGone,
	#[error("Failed to connect to server at {address:?}: {errors:?}")]
	ConnectionFailed { address: String, errors: Vec<Error> },
	#[error("Failed to resolve address: {0}")]
	ResolveAddress(#[source] resolver::Error),
	#[error("Io error: {0}")]
	Io(#[source] tokio::io::Error),
}

/// The result of a download request.
///
/// A file download can be started by [`Connection::download_file`].
///
/// [`Connection::download_file`]: struct.Connection.html#method.download_file
#[derive(Debug)]
pub struct FileDownloadResult {
	/// The size of the requested file.
	// TODO The rest of the size when a seek_position is specified?
	pub size: u64,
	/// The stream where the file can be downloaded.
	pub stream: TcpStream,
}

/// The result of an upload request.
///
/// A file upload can be started by [`Connection::upload_file`].
/// [`Connection::upload_file`]: struct.Connection.html#method.upload_file
#[derive(Debug)]
pub struct FileUploadResult {
	/// The size of the already uploaded part when `resume` was set to `true`
	/// in [`Connection::upload_file`].
	///
	/// [`Connection::upload_file`]: struct.Connection.html#method.upload_file
	pub seek_position: u64,
	/// The stream where the file can be uploaded.
	pub stream: TcpStream,
}

/// An event that gets returned by the connection.
///
/// A stream of these events is returned by [`Connection::events`].
///
/// [`Connection::events`]: struct.Connection.html#method.events
#[derive(Debug)]
pub enum StreamItem {
	/// All the incoming events.
	///
	/// If a connection to the server was established this will contain an added
	/// event of a server.
	ConEvents(Vec<events::Event>),
	/// Received an audio packet.
	///
	/// Audio packets can be handled by the [`AudioHandler`], which builds a
	/// queue per client and handles packet loss and jitter.
	///
	/// [`AudioHandler`]: audio/structAudioHandler.html
	#[cfg(feature = "audio")]
	Audio(InAudioBuf),
	/// The needed level.
	IdentityLevelIncreasing(u8),
	/// This event may occur without an `IdentityLevelIncreasing` event before
	/// if a new identity is created because no identity was supplied.
	IdentityLevelIncreased,
	/// The connection timed out or the server shut down. The connection will be
	/// rebuilt automatically.
	DisconnectedTemporarily,
	/// The result of sending a message.
	///
	/// The [`MessageHandle`] is the return value of
	/// [`Connection::send_command`].
	///
	/// [`MessageHandle`]: struct.MessageHandle.html
	/// [`Connection::send_command`]: struct.Connection.html#method.send_command
	MessageResult(MessageHandle, std::result::Result<(), TsError>),
	/// A file download succeeded. This event contains the `TcpStream` where the
	/// file can be downloaded.
	///
	/// The [`FileTransferHandle`] is the return value of
	/// [`Connection::download_file`].
	///
	/// [`FileTransferHandle`]: struct.FileTransferHandle.html
	/// [`Connection::download_file`]: struct.Connection.html#method.download_file
	FileDownload(FileTransferHandle, FileDownloadResult),
	/// A file upload succeeded. This event contains the `TcpStream` where the
	/// file can be uploaded.
	///
	/// The [`FileTransferHandle`] is the return value of
	/// [`Connection::upload_file`].
	///
	/// [`FileTransferHandle`]: struct.FileTransferHandle.html
	/// [`Connection::upload_file`]: struct.Connection.html#method.upload_file
	FileUpload(FileTransferHandle, FileUploadResult),
	/// A file download or upload failed.
	///
	/// This can happen if either the TeamSpeak server denied the file transfer
	/// or the tcp connection failed.
	///
	/// The [`FileTransferHandle`] is the return value of
	/// [`Connection::download_file`] or [`Connection::upload_file`].
	///
	/// [`FileTransferHandle`]: struct.FileTransferHandle.html
	/// [`Connection::download_file`]: struct.Connection.html#method.download_file
	/// [`Connection::upload_file`]: struct.Connection.html#method.upload_file
	FileTransferFailed(FileTransferHandle, Error),
}

/// The `Connection` is the main interaction point with this library.
///
/// It represents a connection to a TeamSpeak server. It will reconnect
/// automatically when the connection times out (though timeout is not yet
/// implemented). It will not reconnect which the client is kicked or banned
/// from the server.
pub struct Connection {
	state: ConnectionState,
	logger: Logger,
	options: ConnectOptions,
	stream_items: VecDeque<Result<StreamItem>>,
}

struct ConnectedConnection {
	client: client::Client,
	cur_return_code: u16,
	cur_file_transfer_id: u16,
	/// If a file stream can be opened, it gets put in here until the tcp
	/// connection is ready and the key is sent.
	///
	/// Afterwards we can directly return a `TcpStream` in the event stream.
	file_transfers: Vec<future::BoxFuture<'static, StreamItem>>,
}

enum ConnectionState {
	Connecting(future::BoxFuture<'static, Result<(client::Client, data::Connection)>>),
	IdentityLevelIncreasing {
		/// We get the improved identity here.
		recv: oneshot::Receiver<std::result::Result<Identity, tsproto::Error>>,
		state: Arc<Mutex<IdentityIncreaseLevelState>>,
	},
	Connected {
		con: ConnectedConnection,
		book: data::Connection,
	},
}

/// A wrapper to poll events from a connection. This is used so a user can drop
/// and filter the stream of events without problems.
struct EventStream<'a>(&'a mut Connection);

enum IdentityIncreaseLevelState {
	Computing,
	/// Set to this state to cancel the computation.
	Canceled,
}

/// The main type of this crate, which represents a connection to a server.
///
/// After creating a connection with [`Connection::new`], there are a few ways
/// of interacting with it. [`get_state`] lets you inspect the other clients and
/// channels on the server. [`get_mut_state`] in additian allowes changing them.
/// The setter methods e.g. for your own nickname or channel send a command to
/// the server. The methods return a handle which can then be used to check if
/// the action succeeded or not.
///
/// The second way of interaction is polling with [`events()`], which returns a
/// stream of [`StreamItem`]s.
///
/// The connection will not do anything unless the event stream is polled. Even
/// sending packets will only happen while polling. Make sure to always wait for
/// events when awaiting other futures.
///
/// # Examples
/// This will open a connection to the TeamSpeak server at `localhost`.
///
/// ```no_run
/// use futures::prelude::*;
/// use tsclientlib::{Connection, ConnectOptions, StreamItem};
///
/// #[tokio::main]
/// async fn main() {
///     let mut con = Connection::new(ConnectOptions::new("localhost")).unwrap();
///     // Wait until connected
///     con.events()
///         // We are connected when we receive the first ConEvents
///         .try_filter(|e| future::ready(matches!(e, StreamItem::ConEvents(_))))
///         .next()
///         .await
///         .unwrap();
/// }
/// ```
///
/// [`Connection::new`]: #method.new
/// [`get_state`]: #method.get_state
/// [`get_mut_state`]: #method.get_mut_state
/// [`events()`]: #method.events
/// [`StreamItem`]: enum.StreamItem.html
impl Connection {
	/// Create a connection
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
	/// # use futures::prelude::*;
	/// # use tsclientlib::{Connection, ConnectOptions, StreamItem};
	///
	/// # #[tokio::main]
	/// # async fn main() {
	///     let mut con = Connection::new(ConnectOptions::new("localhost")).unwrap();
	///     // Wait until connected
	///     con.events()
	///         // We are connected when we receive the first ConEvents
	///         .try_filter(|e| future::ready(matches!(e, StreamItem::ConEvents(_))))
	///         .next()
	///         .await
	///         .unwrap();
	/// # }
	/// ```
	///
	/// [`ConnectOptions`]: struct.ConnectOptions.html
	pub fn new(mut options: ConnectOptions) -> Result<Self> {
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

		let mut stream_items = VecDeque::new();
		options.identity = Some(
			options
				.identity
				.take()
				.map(Ok)
				.unwrap_or_else(|| {
					// Create new ECDH key
					let id = Identity::create();
					if id.is_ok() {
						// Send event
						stream_items.push_back(Ok(StreamItem::IdentityLevelIncreased));
					}
					id
				})
				.map_err(Error::IdentityCreate)?,
		);

		// Try all addresses
		let fut = Self::connect(logger.clone(), options.clone());

		Ok(Self {
			state: ConnectionState::Connecting(Box::pin(fut)),
			logger,
			options,
			stream_items,
		})
	}

	/// Get the options which were used to create this connection.
	///
	/// The identity of the options is updated while connecting if the identity
	/// level needs to be improved.
	pub fn get_options(&self) -> &ConnectOptions { &self.options }

	/// Get a stream of events. You need to poll the event stream, otherwise
	/// nothing will happen in a connection, not even sending packets will work.
	///
	/// The returned stream can be dropped and recreated if needed.
	pub fn events<'a>(&'a mut self) -> impl Stream<Item = Result<StreamItem>> + 'a {
		EventStream(self)
	}

	async fn connect(
		logger: Logger, options: ConnectOptions,
	) -> Result<(client::Client, data::Connection)> {
		let resolved = match &options.address {
			ServerAddress::SocketAddr(a) => stream::once(future::ok(*a)).left_stream(),
			ServerAddress::Other(s) => resolver::resolve(logger.clone(), s.into()).right_stream(),
		};
		pin_utils::pin_mut!(resolved);
		let mut resolved: Pin<_> = resolved;

		let mut errors = Vec::new();
		while let Some(addr) = resolved.next().await {
			let addr = addr.map_err(Error::ResolveAddress)?;
			match Self::connect_to(&logger, &options, addr).await {
				Ok(res) => return Ok(res),
				Err(e @ Error::IdentityLevel(_)) | Err(e @ Error::ConnectTs(_)) => {
					// Either increase identity level or the server refused us
					return Err(e);
				}
				Err(e) => {
					info!(logger, "Connecting failed, trying next address";
						"error" => %e);
					errors.push(e);
				}
			}
		}
		Err(Error::ConnectionFailed { address: options.address.to_string(), errors })
	}

	async fn connect_to(
		logger: &Logger, options: &ConnectOptions, addr: SocketAddr,
	) -> Result<(client::Client, data::Connection)> {
		let counter = options.identity.as_ref().unwrap().counter();
		let socket = Box::new(
			UdpSocket::bind(options.local_address.unwrap_or_else(|| {
				if addr.is_ipv4() {
					"0.0.0.0:0".parse().unwrap()
				} else {
					"[::]:0".parse().unwrap()
				}
			}))
			.await
			.map_err(Error::Io)?,
		);
		let mut client = client::Client::new(
			logger.clone(),
			addr,
			socket,
			options.identity.as_ref().unwrap().key().clone(),
		);

		// Logging
		let verbosity = if options.log_packets {
			3
		} else if options.log_udp_packets {
			2
		} else if options.log_commands {
			1
		} else {
			0
		};
		if verbosity > 0 {
			tsproto::log::add_logger(logger.clone(), verbosity, &mut *client);
		}

		// Create a connection
		debug!(logger, "Connecting"; "address" => %addr);
		client.connect().await.map_err(Error::Connect)?;

		// Create clientinit packet
		let client_version = options.version.get_version_string();
		let client_platform = options.version.get_platform();
		let client_version_sign = base64::encode(options.version.get_signature());

		let packet = c2s::OutClientInitMessage::new(&mut iter::once(c2s::OutClientInitPart {
			name: &options.name,
			client_version: &client_version,
			client_platform: &client_platform,
			input_hardware_enabled: true,
			output_hardware_enabled: true,
			default_channel: options.channel.as_ref().map(AsRef::as_ref).unwrap_or_default(),
			default_channel_password: options
				.channel_password
				.as_ref()
				.map(AsRef::as_ref)
				.unwrap_or_default(),
			password: options.password.as_ref().map(AsRef::as_ref).unwrap_or_default(),
			metadata: "",
			client_version_sign: &client_version_sign,
			client_key_offset: counter,
			phonetic_name: "",
			default_token: "",
			hardware_id: &options.hardware_id,
			badges: None,
			signed_badges: None,
			integrations: None,
			active_integrations_info: None,
			my_team_speak_avatar: None,
			my_team_speak_id: None,
			security_hash: None,
		}));

		client.send_packet(packet.into_packet()).map_err(Error::SendClientinit)?;

		match time::timeout(
			time::Duration::from_secs(INITSERVER_TIMEOUT),
			Self::wait_initserver(logger, client),
		)
		.await
		{
			Ok(r) => r,
			Err(_) => Err(Error::InitserverTimeout),
		}
	}

	async fn wait_initserver(
		logger: &Logger, mut client: client::Client,
	) -> Result<(client::Client, data::Connection)> {
		// Wait until we received the initserver packet.
		loop {
			let cmd = client
				.filter_commands(|_, cmd| Ok(Some(cmd)))
				.await
				.map_err(Error::InitserverWait)?;
			let msg =
				InMessage::new(logger, cmd.data().packet().header(), cmd.data().packet().content())
					.map_err(Error::InitserverParse);

			match msg {
				Ok(InMessage::CommandError(e)) => {
					let e = e.iter().next().unwrap();
					if e.id == ts_bookkeeping::TsError::ClientCouldNotValidateIdentity {
						if let Some(needed) =
							e.extra_message.as_ref().and_then(|m| m.parse::<u8>().ok())
						{
							return Err(Error::IdentityLevel(needed));
						}
					}
					return Err(Error::ConnectTs(e.id));
				}
				Ok(InMessage::InitServer(initserver)) => {
					let public_key = {
						let params = if let Some(r) = &client.params {
							r
						} else {
							return Err(Error::InitserverParamsMissing);
						};

						params.public_key.clone()
					};

					// Create connection
					let data = data::Connection::new(public_key, &initserver);

					return Ok((client, data));
				}
				Ok(msg) => {
					// TODO Save instead of drop
					warn!(logger, "Expected initserver, dropping command"; "message" => ?msg);
				}
				Err(e) => {
					warn!(logger, "Expected initserver, failed to parse command"; "error" => %e);
				}
			}
		}
	}

	/// If this returns `None`, the level was increased and we should try
	/// connecting again.
	fn increase_identity_level(&mut self, needed: u8) -> Result<()> {
		if needed > 20 {
			return Err(Error::IdentityLevel(needed));
		}

		let identity = self.options.identity.as_ref().unwrap().clone();
		let level = identity.level().map_err(Error::IdentityLevelIncreaseFailed)?;
		if level >= needed {
			return Err(Error::IdentityLevelCorrupted { needed, have: level });
		}

		// Increase identity level
		let state = Arc::new(Mutex::new(IdentityIncreaseLevelState::Computing));
		let (send, recv) = oneshot::channel();
		// TODO Time estimate
		std::thread::spawn(move || {
			let mut identity = identity;
			// TODO Check if canceled in between
			let r = identity.upgrade_level(needed);
			let _ = send.send(r.map(|()| identity));
		});

		self.state = ConnectionState::IdentityLevelIncreasing { recv, state };
		Ok(())
	}

	/// Cancels the computation to increase the identity level.
	///
	/// This function initiates the cancellation and immediately returns. It
	/// does not wait until the background thread quits.
	///
	/// Does nothing if the identity level is currently not increased.
	pub fn cancel_identity_level_increase(&mut self) {
		if let ConnectionState::IdentityLevelIncreasing { state, .. } = &mut self.state {
			*state.lock().unwrap() = IdentityIncreaseLevelState::Canceled;
		}
	}

	/// Get access to the raw connection.
	///
	/// Fails if the connection is currently not connected to the server.
	#[cfg(feature = "unstable")]
	pub fn get_tsproto_client(&self) -> Result<&client::Client> {
		if let ConnectionState::Connected { con, .. } = &self.state {
			Ok(&con.client)
		} else {
			Err(Error::NotConnected)
		}
	}

	/// Get access to the raw connection.
	///
	/// Fails if the connection is currently not connected to the server.
	#[cfg(feature = "unstable")]
	pub fn get_tsproto_client_mut(&mut self) -> Result<&mut client::Client> {
		if let ConnectionState::Connected { con, .. } = &mut self.state {
			Ok(&mut con.client)
		} else {
			Err(Error::NotConnected)
		}
	}

	/// Returns the public key of the server, fails if disconnected.
	#[cfg(feature = "unstable")]
	pub fn get_server_key(&self) -> Result<tsproto_types::crypto::EccKeyPubP256> {
		self.get_tsproto_client().and_then(|c| {
			if let Some(params) = &c.params {
				Ok(params.public_key.clone())
			} else {
				Err(Error::NotConnected)
			}
		})
	}

	/// Adds a `return_code` to the command and returns if the corresponding
	/// answer is received. If an error occurs, the future will return an error.
	#[cfg(feature = "unstable")]
	pub fn send_command(&mut self, packet: OutCommand) -> Result<MessageHandle> {
		if let ConnectionState::Connected { con, .. } = &mut self.state {
			con.send_command(packet)
		} else {
			Err(Error::NotConnected)
		}
	}

	/// Get the current state of clients and channels of this connection.
	///
	/// Fails if the connection is currently not connected to the server.
	pub fn get_state(&self) -> Result<&data::Connection> {
		if let ConnectionState::Connected { book, .. } = &self.state {
			Ok(book)
		} else {
			Err(Error::NotConnected)
		}
	}

	/// Get a connection where you can change properties.
	///
	/// Changing properties will send a packet to the server, it will not
	/// immediately change the value.
	///
	/// Fails if the connection is currently not connected to the server.
	pub fn get_mut_state(&mut self) -> Result<facades::ConnectionMut> {
		if let ConnectionState::Connected { con, book } = &mut self.state {
			Ok(facades::ConnectionMut { connection: con, inner: book })
		} else {
			Err(Error::NotConnected)
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
	/// # use futures::prelude::*;
	/// # use tsclientlib::{Connection, ConnectOptions, DisconnectOptions, StreamItem};
	///
	/// # #[tokio::main]
	/// # async fn main() {
	/// let mut con = Connection::new(ConnectOptions::new("localhost")).unwrap();
	/// // Wait until connected
	/// con.events()
	///     // We are connected when we receive the first ConEvents
	///     .try_filter(|e| future::ready(matches!(e, StreamItem::ConEvents(_))))
	///     .next()
	///     .await
	///     .unwrap();
	///
	/// // Disconnect
	/// con.disconnect(DisconnectOptions::new()).unwrap();
	/// con.events().for_each(|_| future::ready(())).await;
	/// # }
	/// ```
	///
	/// Specify a reason and a quit message:
	///
	/// ```no_run
	/// # use futures::prelude::*;
	/// # use tsclientlib::{Connection, ConnectOptions, DisconnectOptions, Reason, StreamItem};
	///
	/// # #[tokio::main]
	/// # async fn main() {
	/// let mut con = Connection::new(ConnectOptions::new("localhost")).unwrap();
	/// // Wait until connected
	/// con.events()
	///     // We are connected when we receive the first ConEvents
	///     .try_filter(|e| future::ready(matches!(e, StreamItem::ConEvents(_))))
	///     .next()
	///     .await
	///     .unwrap();
	///
	/// // Disconnect
	/// let options = DisconnectOptions::new()
	///     .reason(Reason::Clientdisconnect)
	///     .message("Away for a while");
	/// con.disconnect(options).unwrap();
	/// con.events().for_each(|_| future::ready(())).await;
	/// # }
	/// ```
	pub fn disconnect(&mut self, options: DisconnectOptions) -> Result<()> {
		if let ConnectionState::Connected { con, book } = &mut self.state {
			let packet = book.disconnect(options);
			con.client.send_packet(packet.into_packet()).map_err(Error::SendPacket)?;
		}
		Ok(())
	}

	/// Download a file from a channel of the connected TeamSpeak server.
	///
	/// Returns the size of the file and a tcp stream of the requested file.
	///
	/// # Example
	/// Download an icon.
	///
	/// ```no_run
	/// # use tsclientlib::ChannelId;
	/// # let con: tsclientlib::Connection = panic!();
	/// # let id = 0;
	/// let handle_future = con.download_file(ChannelId(0), &format!("/icon_{}", id), None, None);
	/// # // TODO Show rest of download
	/// ```
	pub fn download_file(
		&mut self, channel_id: ChannelId, path: &str, channel_password: Option<&str>,
		seek_position: Option<u64>,
	) -> Result<FileTransferHandle>
	{
		if let ConnectionState::Connected { con, .. } = &mut self.state {
			con.download_file(channel_id, path, channel_password, seek_position)
		} else {
			Err(Error::NotConnected)
		}
	}

	/// Upload a file to a channel of the connected TeamSpeak server.
	///
	/// Returns the size of the part which is already uploaded (when resume is
	/// specified) and a tcp stream where the requested file should be uploaded.
	///
	/// # Example
	/// Upload an avatar.
	///
	/// ```no_run
	/// # use tsclientlib::ChannelId;
	/// # let con: tsclientlib::Connection = panic!();
	/// # let size = 0;
	/// let handle_future = con.upload_file(ChannelId(0), "/avatar", None, size, true, false);
	/// # // TODO Show rest of upload
	/// ```
	pub fn upload_file(
		&mut self, channel_id: ChannelId, path: &str, channel_password: Option<&str>, size: u64,
		overwrite: bool, resume: bool,
	) -> Result<FileTransferHandle>
	{
		if let ConnectionState::Connected { con, .. } = &mut self.state {
			con.upload_file(channel_id, path, channel_password, size, overwrite, resume)
		} else {
			Err(Error::NotConnected)
		}
	}

	fn poll_next(&mut self, cx: &mut Context) -> Poll<Option<Result<StreamItem>>> {
		if let Some(item) = self.stream_items.pop_front() {
			return Poll::Ready(Some(item));
		}
		match &mut self.state {
			ConnectionState::Connecting(fut) => match fut.poll_unpin(cx) {
				Poll::Pending => Poll::Pending,
				Poll::Ready(Err(Error::IdentityLevel(level))) => {
					if let Err(e) = self.increase_identity_level(level) {
						return Poll::Ready(Some(Err(e)));
					}
					Poll::Ready(Some(Ok(StreamItem::IdentityLevelIncreasing(level))))
				}
				Poll::Ready(Err(e)) => Poll::Ready(Some(Err(e))),
				Poll::Ready(Ok((client, book))) => {
					let con = ConnectedConnection {
						client,
						cur_return_code: 0,
						cur_file_transfer_id: 0,
						file_transfers: Default::default(),
					};
					self.state = ConnectionState::Connected { con, book };
					Poll::Ready(Some(Ok(StreamItem::ConEvents(vec![
						events::Event::PropertyAdded {
							id: events::PropertyId::Server,
							invoker: None,
							extra: Default::default(),
						},
					]))))
				}
			},
			ConnectionState::IdentityLevelIncreasing { recv, .. } => match recv.poll_unpin(cx) {
				Poll::Pending => Poll::Pending,
				Poll::Ready(Err(_)) => {
					Poll::Ready(Some(Err(Error::IdentityLevelIncreaseFailedThread)))
				}
				Poll::Ready(Ok(Err(e))) => {
					Poll::Ready(Some(Err(Error::IdentityLevelIncreaseFailed(e))))
				}
				Poll::Ready(Ok(Ok(identity))) => {
					self.options.identity = Some(identity);
					let fut = Self::connect(self.logger.clone(), self.options.clone());
					self.state = ConnectionState::Connecting(Box::pin(fut));
					Poll::Ready(Some(Ok(StreamItem::IdentityLevelIncreased)))
				}
			},
			ConnectionState::Connected { con, book } => match loop {
				match con.client.poll_next_unpin(cx) {
					Poll::Pending => break Poll::Pending,
					Poll::Ready(None) => break Poll::Ready(None),
					Poll::Ready(Some(Err(e))) => {
						// Check if we were disconnecting
						let state = con.client.resender.get_state();
						if state == ResenderState::Disconnected
							|| state == ResenderState::Disconnecting
						{
							break Poll::Ready(None);
						}

						warn!(self.logger, "Connection failed, reconnecting"; "error" => %e);
						// Reconnect
						// TODO Depending on reason
						let fut = Self::connect(self.logger.clone(), self.options.clone());
						self.state = ConnectionState::Connecting(Box::pin(fut));
						return Poll::Ready(Some(Ok(StreamItem::DisconnectedTemporarily)));
					}
					Poll::Ready(Some(Ok(item))) => match item {
						ProtoStreamItem::Error(e) => {
							warn!(self.logger, "Connection got a non-fatal error";
									"error" => %e);
						}
						ProtoStreamItem::Audio(audio) => {
							#[cfg(feature = "audio")]
							{
								return Poll::Ready(Some(Ok(StreamItem::Audio(audio))));
							}
							#[cfg(not(feature = "audio"))]
							{
								let _ = audio;
							}
						}
						ProtoStreamItem::Command(cmd) => {
							con.handle_command(&self.logger, book, &mut self.stream_items, cmd);
							if let Some(item) = self.stream_items.pop_front() {
								break Poll::Ready(Some(item));
							}
						}
						_ => {}
					},
				}
			} {
				Poll::Ready(r) => Poll::Ready(r),
				Poll::Pending => {
					// Check file transfers
					let ft = con.file_transfers.iter_mut().enumerate().find_map(|(i, ft)| match ft
						.poll_unpin(cx)
					{
						Poll::Pending => None,
						Poll::Ready(r) => Some((i, r)),
					});
					if let Some((i, res)) = ft {
						con.file_transfers.remove(i);
						Poll::Ready(Some(Ok(res)))
					} else {
						Poll::Pending
					}
				}
			},
		}
	}
}

impl Drop for Connection {
	fn drop(&mut self) { self.cancel_identity_level_increase(); }
}

impl<'a> Stream for EventStream<'a> {
	type Item = Result<StreamItem>;
	fn poll_next(mut self: Pin<&mut Self>, ctx: &mut Context) -> Poll<Option<Self::Item>> {
		self.0.poll_next(ctx)
	}
}

impl ConnectedConnection {
	fn handle_command(
		&mut self, logger: &Logger, book: &mut data::Connection,
		stream_items: &mut VecDeque<Result<StreamItem>>, cmd: InCommandBuf,
	)
	{
		let msg = match InMessage::new(
			logger,
			&cmd.data().packet().header(),
			&cmd.data().packet().content(),
		) {
			Ok(r) => r,
			Err(e) => {
				warn!(logger, "Failed to parse message"; "error" => %e);
				return;
			}
		};

		// Handle error messages
		if let InMessage::CommandError(e) = &msg {
			for e in e.iter() {
				if let Some(ret_code) = e.return_code.as_ref().and_then(|r| r.parse().ok()) {
					let res = if e.id == TsError::Ok { Ok(()) } else { Err(e.id) };
					stream_items
						.push_back(Ok(StreamItem::MessageResult(MessageHandle(ret_code), res)));
				}
			}
		} else if let InMessage::FileDownload(msg) = &msg {
			for msg in msg.iter() {
				let ft_id = FileTransferHandle(msg.client_file_transfer_id);
				let ip = msg.ip.unwrap_or_else(|| self.client.address.ip());
				let addr = SocketAddr::new(ip, msg.port);
				let key = msg.file_transfer_key.clone();
				let size = msg.size;

				let fut = Box::new(async move {
					let addr = addr;
					let key = key;
					let mut stream =
						TcpStream::connect(&addr).await.map_err(Error::FileTransferIo)?;
					stream.write_all(key.as_bytes()).await.map_err(Error::FileTransferIo)?;
					stream.flush().await.map_err(Error::FileTransferIo)?;
					Ok(stream)
				})
				.map(move |res| match res {
					Ok(stream) => {
						StreamItem::FileDownload(ft_id, FileDownloadResult { size, stream })
					}
					Err(e) => StreamItem::FileTransferFailed(ft_id, e),
				});

				self.file_transfers.push(Box::pin(fut));
			}
		} else if let InMessage::FileUpload(msg) = &msg {
			for msg in msg.iter() {
				let ft_id = FileTransferHandle(msg.client_file_transfer_id);
				let ip = msg.ip.unwrap_or_else(|| self.client.address.ip());
				let addr = SocketAddr::new(ip, msg.port);
				let key = msg.file_transfer_key.clone();
				let seek_position = msg.seek_position;

				let fut = Box::new(async move {
					let addr = addr;
					let key = key;
					let mut stream =
						TcpStream::connect(&addr).await.map_err(Error::FileTransferIo)?;
					stream.write_all(key.as_bytes()).await.map_err(Error::FileTransferIo)?;
					stream.flush().await.map_err(Error::FileTransferIo)?;
					Ok(stream)
				})
				.map(move |res| match res {
					Ok(stream) => {
						StreamItem::FileUpload(ft_id, FileUploadResult { seek_position, stream })
					}
					Err(e) => StreamItem::FileTransferFailed(ft_id, e),
				});

				self.file_transfers.push(Box::pin(fut));
			}
		} else if let InMessage::FileTransferStatus(msg) = &msg {
			for msg in msg.iter() {
				let ft_id = FileTransferHandle(msg.client_file_transfer_id);
				stream_items
					.push_back(Ok(StreamItem::FileTransferFailed(ft_id, msg.status.into())));
			}
		} else {
			let events = match book.handle_command(logger, &msg) {
				Ok(r) => r,
				Err(e) => {
					warn!(logger, "Failed to handle message"; "error" => %e);
					return;
				}
			};
			self.client.hand_back_buffer(cmd.into_buffer());
			if !events.is_empty() {
				stream_items.push_back(Ok(StreamItem::ConEvents(events)));
			}
		}
	}

	fn send_command(&mut self, mut packet: OutCommand) -> Result<MessageHandle> {
		let code = self.cur_return_code;
		self.cur_return_code += 1;
		packet.write_arg("return_code", &code);
		self.client
			.send_packet(packet.into_packet())
			.map(|_| MessageHandle(code))
			.map_err(Error::SendPacket)
	}

	fn download_file(
		&mut self, channel_id: ChannelId, path: &str, channel_password: Option<&str>,
		seek_position: Option<u64>,
	) -> Result<FileTransferHandle>
	{
		let ft_id = self.cur_file_transfer_id;
		self.cur_file_transfer_id += 1;
		let packet =
			c2s::OutFtInitDownloadMessage::new(&mut iter::once(c2s::OutFtInitDownloadPart {
				client_file_transfer_id: ft_id,
				name: path,
				channel_id,
				channel_password: channel_password.unwrap_or(""),
				seek_position: seek_position.unwrap_or_default(),
				protocol: 1,
			}));

		self.send_command(packet).map(|_| FileTransferHandle(ft_id))
	}

	fn upload_file(
		&mut self, channel_id: ChannelId, path: &str, channel_password: Option<&str>, size: u64,
		overwrite: bool, resume: bool,
	) -> Result<FileTransferHandle>
	{
		let ft_id = self.cur_file_transfer_id;
		self.cur_file_transfer_id += 1;

		let packet = c2s::OutFtInitUploadMessage::new(&mut iter::once(c2s::OutFtInitUploadPart {
			client_file_transfer_id: ft_id,
			name: path,
			channel_id,
			channel_password: channel_password.unwrap_or(""),
			overwrite,
			resume,
			size,
			protocol: 1,
		}));

		self.send_command(packet).map(|_| FileTransferHandle(ft_id))
	}
}

/// The configuration for creating a new connection.
///
/// # Example
///
/// ```
/// # use tsclientlib::{Connection, ConnectOptions};
/// let config = ConnectOptions::new("localhost")
///     .name("MyUser")
///     .channel("Default Channel/Nested");
///
/// let con = Connection::new(config);
/// ```
#[derive(Clone, Debug)]
pub struct ConnectOptions {
	address: ServerAddress,
	local_address: Option<SocketAddr>,
	identity: Option<Identity>,
	name: Cow<'static, str>,
	version: Version,
	hardware_id: Cow<'static, str>,
	channel: Option<Cow<'static, str>>,
	channel_password: Option<Cow<'static, str>>,
	password: Option<Cow<'static, str>>,
	logger: Option<Logger>,
	log_commands: bool,
	log_packets: bool,
	log_udp_packets: bool,
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
			name: "TeamSpeakUser".into(),
			version: Version::Windows_3_X_X__1,
			hardware_id: "923f136fb1e22ae6ce95e60255529c00,d13231b1bc33edfecfb9169cc7a63bcc".into(),
			channel: None,
			channel_password: None,
			password: None,
			logger: None,
			log_commands: false,
			log_packets: false,
			log_udp_packets: false,
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
	pub fn name<S: Into<Cow<'static, str>>>(mut self, name: S) -> Self {
		self.name = name.into();
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

	/// The hardware ID (HWID) of the client.
	///
	/// # Default
	/// `923f136fb1e22ae6ce95e60255529c00,d13231b1bc33edfecfb9169cc7a63bcc`
	#[inline]
	pub fn hardware_id<S: Into<Cow<'static, str>>>(mut self, hwid: S) -> Self {
		self.hardware_id = hwid.into();
		self
	}

	/// Connect to a specific channel.
	///
	/// # Example
	/// ```
	/// # use tsclientlib::ConnectOptions;
	/// let opts = ConnectOptions::new("localhost").channel("Default Channel");
	/// ```
	///
	/// Connecting to a channel further down in the hierarchy.
	/// ```
	/// # use tsclientlib::ConnectOptions;
	/// let opts = ConnectOptions::new("localhost")
	///     .channel("Default Channel/Nested");
	/// ```
	#[inline]
	pub fn channel<S: Into<Cow<'static, str>>>(mut self, path: S) -> Self {
		self.channel = Some(path.into());
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
		self.channel = Some(format!("/{}", channel.0).into());
		self
	}

	/// Use a password for the given channel when connecting.
	///
	/// # Example
	/// ```
	/// # use tsclientlib::ConnectOptions;
	/// let opts = ConnectOptions::new("localhost")
	///     .channel("Secret Channel")
	///     .channel_password("My secret password");
	/// ```
	#[inline]
	pub fn channel_password<S: Into<Cow<'static, str>>>(mut self, pwd: S) -> Self {
		self.channel_password = Some(pwd.into());
		self
	}

	/// Use a server password when connecting.
	///
	/// # Example
	/// ```
	/// # use tsclientlib::ConnectOptions;
	/// let opts = ConnectOptions::new("localhost").password("My secret password");
	/// ```
	#[inline]
	pub fn password<S: Into<Cow<'static, str>>>(mut self, pwd: S) -> Self {
		self.password = Some(pwd.into());
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

	#[inline]
	pub fn get_address(&self) -> &ServerAddress { &self.address }
	#[inline]
	pub fn get_local_address(&self) -> Option<&SocketAddr> { self.local_address.as_ref() }
	#[inline]
	pub fn get_identity(&self) -> Option<&Identity> { self.identity.as_ref() }
	#[inline]
	pub fn get_name(&self) -> &str { &self.name }
	#[inline]
	pub fn get_version(&self) -> &Version { &self.version }
	#[inline]
	pub fn get_hardware_id(&self) -> &str { &self.hardware_id }
	#[inline]
	pub fn get_channel(&self) -> Option<&str> { self.channel.as_ref().map(AsRef::as_ref) }
	#[inline]
	pub fn get_channel_password(&self) -> Option<&str> {
		self.channel_password.as_ref().map(AsRef::as_ref)
	}
	#[inline]
	pub fn get_password(&self) -> Option<&str> { self.password.as_ref().map(AsRef::as_ref) }
	#[inline]
	pub fn get_logger(&self) -> Option<&Logger> { self.logger.as_ref() }
	#[inline]
	pub fn get_log_commands(&self) -> bool { self.log_commands }
	#[inline]
	pub fn get_log_packets(&self) -> bool { self.log_packets }
	#[inline]
	pub fn get_log_udp_packets(&self) -> bool { self.log_udp_packets }
}
