//! tsclientlib is a library which makes it simple to create TeamSpeak clients
//! and bots.
//!
//! For a full client application, you might want to have a look at [Qint].
//!
//! If more power over the internals of a connection is needed, the `unstable` feature can be
//! enabled. Beware that functionality behind this feature may change on any minor release.
//!
//! The base class of this library is the [`Connection`]. One instance of this
//! struct manages a single connection to a server.
//!
//! [Qint]: https://github.com/ReSpeak/Qint
// Needed for futures on windows.
#![recursion_limit = "128"]

use std::borrow::Cow;
use std::collections::VecDeque;
use std::convert::TryInto;
use std::iter;
use std::mem;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::Duration;

use base64::prelude::*;
use futures::prelude::*;
use thiserror::Error;
use time::OffsetDateTime;
use tokio::io::AsyncWriteExt;
use tokio::net::{TcpStream, UdpSocket};
use tokio::sync::oneshot;
use tracing::{debug, info, info_span, warn, Instrument, Span};
use ts_bookkeeping::messages::c2s;
use ts_bookkeeping::messages::OutMessageTrait;
use tsproto::client;
use tsproto::connection::StreamItem as ProtoStreamItem;
use tsproto::resend::ResenderState;
use tsproto_packets::commands::{CommandItem, CommandParser};
#[cfg(feature = "audio")]
use tsproto_packets::packets::InAudioBuf;
use tsproto_packets::packets::{InCommandBuf, OutCommand, OutPacket, PacketType};

#[cfg(feature = "audio")]
pub mod audio;
pub mod prelude;
pub mod resolver;
pub mod sync;

// The build environment of tsclientlib.
git_testament::git_testament!(TESTAMENT);

#[cfg(test)]
mod tests;

// Reexports
pub use ts_bookkeeping::messages::s2c::InMessage;
// TODO This is bad because it re-exports ConnectOptions
pub use ts_bookkeeping::*;
pub use tsproto::resend::{ConnectionStats, PacketStat};
pub use tsproto::Identity;
pub use tsproto_types::errors::Error as TsError;

/// Wait this time for initserver, in seconds.
const INITSERVER_TIMEOUT: u64 = 5;

type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct MessageHandle(pub u16);
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct FiletransferHandle(pub u16);

#[derive(Clone, Debug, Error, Eq, Hash, PartialEq)]
#[error("{}", error)]
pub struct CommandError {
	#[source]
	pub error: TsError,
	pub missing_permission: Option<Permission>,
}

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
	/// A command return an error.
	#[error(transparent)]
	CommandError(#[from] CommandError),
	#[error("Failed to connect: {0}")]
	Connect(#[source] tsproto::client::Error),
	#[error("Failed to connect to server at {address:?}: {errors:?}")]
	ConnectFailed { address: String, errors: Vec<Error> },
	#[error("Connection aborted: {0}")]
	ConnectionFailed(#[source] tsproto::client::Error),
	/// The connection was destroyed.
	#[error("Connection does not exist anymore")]
	ConnectionGone,
	#[error("Server refused connection: {0}")]
	ConnectTs(#[source] tsproto_types::errors::Error),
	#[error("File transfer failed: {0}")]
	FiletransferIo(#[source] std::io::Error),
	#[error("Failed to create identity: {0}")]
	IdentityCreate(#[source] tsproto::Error),
	#[error("The server needs an identity of level {0}, please increase your identity level")]
	IdentityLevel(u8),
	#[error("The server requested an identity of level {needed}, but we already have level {have}")]
	IdentityLevelCorrupted { needed: u8, have: u8 },
	#[error("Failed to increase identity level: Thread died")]
	IdentityLevelIncreaseFailedThread,
	#[error("We should be connected but the connection params do not exist")]
	InitserverParamsMissing,
	#[error("Failed to parse initserver: {0}")]
	InitserverParse(#[source] ts_bookkeeping::messages::ParseError),
	#[error("Timeout while waiting for initserver")]
	InitserverTimeout,
	#[error("Failed to receive initserver: {0}")]
	InitserverWait(#[source] tsproto::client::Error),
	#[error("Io error: {0}")]
	Io(#[source] tokio::io::Error),
	/// The connection is currently not connected to a server but is in the process of connecting.
	#[error("Currently not connected")]
	NotConnected,
	#[error("Failed to resolve address: {0}")]
	ResolveAddress(#[source] Box<resolver::Error>),
	#[error("Failed to send clientinit: {0}")]
	SendClientinit(#[source] tsproto::client::Error),
	#[error("Failed to send packet: {0}")]
	SendPacket(#[source] tsproto::client::Error),
	#[error("The server changed its identity")]
	ServerUidMismatch(UidBuf),
}

/// The reason for a temporary disconnect.
#[derive(Clone, Copy, Debug)]
pub enum TemporaryDisconnectReason {
	/// Timed out because the server did not respond to packets in time.
	Timeout(&'static str),
	/// The server terminated our connection because it shut down.
	///
	/// This corresponds to the `Serverstop` and `ClientdisconnectServerShutdown` reasons.
	/// This often happens on server restarts, so the connection will try to reconnect.
	Serverstop,
}

pub trait OutCommandExt {
	/// Adds a `return_code` to the command and returns if the corresponding
	/// answer is received. If an error occurs, the future will return an error.
	fn send_with_result(self, con: &mut Connection) -> Result<MessageHandle>;

	/// Sends the command without asking for an answer.
	fn send(self, con: &mut Connection) -> Result<()>;
}

/// The result of a download request.
///
/// A file download can be started by [`Connection::download_file`].
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
#[derive(Debug)]
pub struct FileUploadResult {
	/// The size of the already uploaded part when `resume` was set to `true`
	/// in [`Connection::upload_file`].
	pub seek_position: u64,
	/// The stream where the file can be uploaded.
	pub stream: TcpStream,
}

/// Signals audio related changes.
#[derive(Debug)]
pub enum AudioEvent {
	/// If this client can send audio or is muted.
	///
	/// If the client is muted, the server will drop any sent audio packets. A client counts as
	/// muted if input or output is muted, he is marked away or the talk power is less than the
	/// needed talk power in the current channel. Temporary disconnects also count as mutes.
	///
	/// Every time the mute state changes, this event is emitted.
	CanSendAudio(bool),
	/// If this client can receive audio or the output is muted.
	///
	/// Audio packets might still be received but should not be played.
	///
	/// Every time the mute state changes, this event is emitted.
	CanReceiveAudio(bool),
}

/// An event that gets returned by the connection.
///
/// A stream of these events is returned by [`Connection::events`].
#[derive(Debug)]
pub enum StreamItem {
	/// All the incoming book events.
	///
	/// If a connection to the server was established this will contain an added event of a server.
	BookEvents(Vec<events::Event>),
	/// All incoming messages that are not related to the book.
	///
	/// This contains messages like `ChannelListFinished` or `ClientChatComposing`.
	/// All events related to channels or clients are returned as events in the `BookEvents`
	/// variant. Other messages handled by tsclientlib, e.g. for filetransfer are also not included
	/// in these events.
	MessageEvent(InMessage),
	/// Received an audio packet.
	///
	/// Audio packets can be handled by the [`AudioHandler`](audio::AudioHandler), which builds a
	/// queue per client and handles packet loss and jitter.
	#[cfg(feature = "audio")]
	Audio(InAudioBuf),
	/// The needed level.
	IdentityLevelIncreasing(u8),
	/// This event may occur without an `IdentityLevelIncreasing` event before
	/// if a new identity is created because no identity was supplied.
	IdentityLevelIncreased,
	/// The connection timed out or the server shut down. The connection will be
	/// rebuilt automatically.
	DisconnectedTemporarily(TemporaryDisconnectReason),
	/// The result of sending a message.
	///
	/// The [`MessageHandle`] is the return value of [`OutCommandExt::send_with_result`].
	MessageResult(MessageHandle, std::result::Result<(), CommandError>),
	/// A file download succeeded. This event contains the `TcpStream` where the
	/// file can be downloaded.
	///
	/// The [`FiletransferHandle`] is the return value of [`Connection::download_file`].
	FileDownload(FiletransferHandle, FileDownloadResult),
	/// A file upload succeeded. This event contains the `TcpStream` where the
	/// file can be uploaded.
	///
	/// The [`FiletransferHandle`] is the return value of [`Connection::upload_file`].
	FileUpload(FiletransferHandle, FileUploadResult),
	/// A file download or upload failed.
	///
	/// This can happen if either the TeamSpeak server denied the file transfer or the tcp
	/// connection failed.
	///
	/// The [`FiletransferHandle`] is the return value of [`Connection::download_file`] or
	/// [`Connection::upload_file`].
	FiletransferFailed(FiletransferHandle, Error),
	/// The network statistics were updated.
	///
	/// This means e.g. the packet loss got a new value. Clients with audio probably want to update
	/// the packet loss option of opus.
	NetworkStatsUpdated,
	/// A change related to audio.
	AudioChange(AudioEvent),
}

/// The `Connection` is the main interaction point with this library.
///
/// It represents a connection to a TeamSpeak server. It will reconnect automatically when the
/// connection times out. It will not reconnect which the client is kicked or banned from the
/// server.
pub struct Connection {
	state: ConnectionState,
	span: Span,
	options: ConnectOptions,
	stream_items: VecDeque<Result<StreamItem>>,
}

struct ConnectedConnection {
	client: client::Client,
	cur_return_code: u16,
	cur_filetransfer_id: u16,
	/// If we are subscribed to the server. This will automatically subscribe to new channels.
	subscribed: bool,
	/// If a file stream can be opened, it gets put in here until the tcp
	/// connection is ready and the key is sent.
	///
	/// Afterwards we can directly return a `TcpStream` in the event stream.
	filetransfers: Vec<future::BoxFuture<'static, StreamItem>>,
	connection_time: time::OffsetDateTime,
}

enum ConnectionState {
	/// The future that resolves to a connection and a boolean if we should reconnect on failure.
	///
	/// If the `bool` is `false`, the connection will abort on error. Otherwise it will try to
	/// connect again on timeout.
	Connecting(future::BoxFuture<'static, Result<(client::Client, data::Connection)>>, bool),
	IdentityLevelIncreasing {
		/// We get the improved identity here.
		recv: oneshot::Receiver<Identity>,
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
/// After creating a connection with [`Connection::new`], the main way to interact with it is
/// [`get_state`](Connection::get_state). It stores currently visible clients and channels on the
/// server.  The setter methods e.g. for the nickname or channel create a command that can be send
/// to the server. The send method returns a handle which can then be used to check if
/// the action succeeded or not.
///
/// The second way of interaction is polling with [`events()`](Connection::events), which returns a
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
/// use tsclientlib::{Connection, StreamItem};
///
/// #[tokio::main]
/// async fn main() {
///     let mut con = Connection::build("localhost").connect().unwrap();
///     // Wait until connected
///     con.events()
///         // We are connected when we receive the first BookEvents
///         .try_filter(|e| future::ready(matches!(e, StreamItem::BookEvents(_))))
///         .next()
///         .await
///         .unwrap();
/// }
/// ```
impl Connection {
	/// Start creating the configuration of a new connection.
	///
	/// # Arguments
	/// The address of the server has to be supplied. The address can be a
	/// [`SocketAddr`](std::net::SocketAddr), a string or directly a [`ServerAddress`]. A string
	/// will automatically be resolved from all formats supported by TeamSpeak.
	/// For details, see [`resolver::resolve`].
	///
	/// # Examples
	/// This will open a connection to the TeamSpeak server at `localhost`.
	///
	/// ```no_run
	/// # use futures::prelude::*;
	/// # use tsclientlib::{Connection, StreamItem};
	///
	/// # #[tokio::main]
	/// # async fn main() {
	///     let mut con = Connection::build("localhost").connect().unwrap();
	///     // Wait until connected
	///     con.events()
	///         // We are connected when we receive the first BookEvents
	///         .try_filter(|e| future::ready(matches!(e, StreamItem::BookEvents(_))))
	///         .next()
	///         .await
	///         .unwrap();
	/// # }
	/// ```
	#[inline]
	pub fn build<A: Into<ServerAddress>>(address: A) -> ConnectOptions {
		ConnectOptions {
			address: address.into(),
			local_address: None,
			identity: None,
			server: None,
			name: "TeamSpeakUser".into(),
			version: Version::Windows_3_X_X__1,
			hardware_id: "923f136fb1e22ae6ce95e60255529c00,d13231b1bc33edfecfb9169cc7a63bcc".into(),
			channel: None,
			channel_password: None,
			password: None,
			input_muted: false,
			output_muted: false,
			input_hardware_enabled: true,
			output_hardware_enabled: true,
			away: None,
			log_commands: false,
			log_packets: false,
			log_udp_packets: false,
		}
	}

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
	///         // We are connected when we receive the first BookEvents
	///         .try_filter(|e| future::ready(matches!(e, StreamItem::BookEvents(_))))
	///         .next()
	///         .await
	///         .unwrap();
	/// # }
	/// ```
	// TODO Remove
	#[deprecated(since = "0.2.0", note = "ConnectOptions::connect should be used instead")]
	pub fn new(options: ConnectOptions) -> Result<Self> { options.connect() }

	/// Get the options which were used to create this connection.
	///
	/// The identity of the options is updated while connecting if the identity
	/// level needs to be improved.
	pub fn get_options(&self) -> &ConnectOptions { &self.options }

	/// Get a stream of events. The event stream needs to be polled, otherwise
	/// nothing will happen in a connection, not even sending packets will work.
	///
	/// The returned stream can be dropped and recreated if needed.
	pub fn events(&mut self) -> impl Stream<Item = Result<StreamItem>> + '_ { EventStream(self) }

	/// Connect to a server.
	///
	/// If `is_reconnect` is `true`, wait 10 seconds before sending the first packet. This is to not
	/// spam unnecessary packets if the internet or server is down.
	async fn connect(
		options: ConnectOptions, is_reconnect: bool,
	) -> Result<(client::Client, data::Connection)> {
		if is_reconnect {
			tokio::time::sleep(Duration::from_secs(10)).await;
		}

		let resolved = match &options.address {
			ServerAddress::SocketAddr(a) => stream::once(future::ok(*a)).left_stream(),
			ServerAddress::Other(s) => resolver::resolve(s.into()).right_stream(),
		};
		pin_utils::pin_mut!(resolved);
		let mut resolved: Pin<_> = resolved;

		let mut errors = Vec::new();
		while let Some(addr) = resolved.next().await {
			let addr = addr.map_err(|e| Error::ResolveAddress(Box::new(e)))?;
			match Self::connect_to(&options, addr).await {
				Ok(res) => return Ok(res),
				Err(e @ Error::IdentityLevel(_)) | Err(e @ Error::ConnectTs(_)) => {
					// Either increase identity level or the server refused us
					return Err(e);
				}
				Err(error) => {
					info!(%error, "Connecting failed, trying next address");
					errors.push(error);
				}
			}
		}
		Err(Error::ConnectFailed { address: options.address.to_string(), errors })
	}

	async fn connect_to(
		options: &ConnectOptions, addr: SocketAddr,
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
		let mut client =
			client::Client::new(addr, socket, options.identity.as_ref().unwrap().key().clone());

		// Logging
		tsproto::log::add_logger(
			options.log_commands,
			options.log_packets,
			options.log_udp_packets,
			&mut *client,
		);

		// Create a connection
		debug!(address = %addr, "Connecting");
		client.connect().await.map_err(Error::Connect)?;

		if let Some(server_uid) = &options.server {
			let params = if let Some(r) = &client.params {
				r
			} else {
				return Err(Error::InitserverParamsMissing);
			};
			let real_uid = UidBuf(params.public_key.get_uid_no_base64());
			if real_uid != *server_uid {
				return Err(Error::ServerUidMismatch(real_uid));
			}
		}

		// Create clientinit packet
		let client_version = options.version.get_version_string();
		let client_platform = options.version.get_platform();
		let client_version_sign = BASE64_STANDARD.encode(options.version.get_signature());

		let default_channel_password = options
			.channel_password
			.as_ref()
			.map(|p| tsproto_types::crypto::encode_password(p.as_bytes()))
			.unwrap_or_default();
		let password = options
			.password
			.as_ref()
			.map(|p| tsproto_types::crypto::encode_password(p.as_bytes()))
			.unwrap_or_default();
		let packet = c2s::OutClientInitMessage::new(&mut iter::once(c2s::OutClientInitPart {
			name: Cow::Borrowed(options.name.as_ref()),
			version: Cow::Borrowed(client_version),
			platform: Cow::Borrowed(client_platform),
			input_muted: if options.input_muted { Some(true) } else { None },
			output_muted: if options.output_muted { Some(true) } else { None },
			input_hardware_enabled: options.input_hardware_enabled,
			output_hardware_enabled: options.output_hardware_enabled,
			is_away: if options.away.is_some() { Some(true) } else { None },
			away_message: options.away.as_deref().map(Cow::Borrowed),
			default_channel: Cow::Borrowed(options.channel.as_deref().unwrap_or_default()),
			default_channel_password: Cow::Borrowed(default_channel_password.as_ref()),
			password: Cow::Borrowed(password.as_ref()),
			metadata: "".into(),
			version_sign: Cow::Borrowed(client_version_sign.as_ref()),
			client_key_offset: counter,
			phonetic_name: "".into(),
			default_token: "".into(),
			hardware_id: Cow::Borrowed(options.hardware_id.as_ref()),
			badges: None,
			signed_badges: None,
			integrations: None,
			active_integrations_info: None,
			my_team_speak_avatar: None,
			my_team_speak_id: None,
			security_hash: None,
		}));

		client.send_packet(packet.into_packet()).map_err(Error::SendClientinit)?;

		match tokio::time::timeout(
			Duration::from_secs(INITSERVER_TIMEOUT),
			Self::wait_initserver(client),
		)
		.await
		{
			Ok(r) => r,
			Err(_) => Err(Error::InitserverTimeout),
		}
	}

	async fn wait_initserver(
		mut client: client::Client,
	) -> Result<(client::Client, data::Connection)> {
		// Wait until we received the initserver packet.
		loop {
			let cmd = client
				.filter_commands(|_, cmd| Ok(Some(cmd)))
				.await
				.map_err(Error::InitserverWait)?;
			let msg = InMessage::new(cmd.data().packet().header(), cmd.data().packet().content())
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
					warn!(message = ?msg, "Expected initserver, dropping command");
				}
				Err(error) => {
					warn!(%error, "Expected initserver, failed to parse command");
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
		let level = identity.level();
		if level >= needed {
			return Err(Error::IdentityLevelCorrupted { needed, have: level });
		}

		// Increase identity level
		let state = Arc::new(Mutex::new(IdentityIncreaseLevelState::Computing));
		let (send, recv) = oneshot::channel();
		// TODO Use tokio::blocking
		// TODO Time estimate
		std::thread::spawn(move || {
			let mut identity = identity;
			// TODO Check if canceled in between
			identity.upgrade_level(needed);
			let _ = send.send(identity);
		});

		self.state = ConnectionState::IdentityLevelIncreasing { recv, state };
		Ok(())
	}

	/// Adds a `return_code` to the command and returns if the corresponding
	/// answer is received. If an error occurs, the future will return an error.
	fn send_command_with_result(&mut self, packet: OutCommand) -> Result<MessageHandle> {
		self.update_on_outgoing_command(&packet);
		if let ConnectionState::Connected { con, .. } = &mut self.state {
			con.send_command_with_result(packet)
		} else {
			Err(Error::NotConnected)
		}
	}

	fn send_command(&mut self, packet: OutCommand) -> Result<()> {
		self.update_on_outgoing_command(&packet);
		if let ConnectionState::Connected { con, .. } = &mut self.state {
			con.send_command(packet)
		} else {
			Err(Error::NotConnected)
		}
	}

	/// Update for outgoing commands.
	///
	/// Updates subscription and muted state.
	///
	/// Muted state needs to be handled at the following places:
	/// - Outgoing packet to change mute/away state: Immediately apply and add book event, update
	///   CanTalk, save in ConnectOptions
	/// - Incoming event to change channel/own name, edit current channel/own client permissions,
	///   server settings: Update CanTalk/Play, save in ConnectOptions
	/// - Incoming packet to change mute/away state: Ignore for own client
	/// - Connect, temporary disconnect: Update CanTalk/Play
	fn update_on_outgoing_command(&mut self, cmd: &OutCommand) {
		if let ConnectionState::Connected { con, book } = &mut self.state {
			let (cmd_name, parser) = CommandParser::new(cmd.0.content());

			if cmd_name == b"channelsubscribeall" {
				con.subscribed = true;
			} else if cmd_name == b"channelunsubscribeall" {
				con.subscribed = false;
			} else if cmd_name == b"clientupdate" {
				let prev_can_send = Self::intern_can_send_audio(book, &self.options);
				let prev_can_receive = Self::intern_can_receive_audio(book, &self.options);
				let mut own_client = book.clients.get_mut(&book.own_client);
				let mut has_away_message = false;
				let mut has_is_away = false;
				let mut events = Vec::new();

				for arg in parser {
					if let CommandItem::Argument(arg) = arg {
						macro_rules! set_prop {
							($prop:ident, $event:ident, $event_value:ident) => {
								self.options.$prop = arg.value().get_raw() == b"1";
								if let Some(own_client) = &mut own_client {
									if own_client.$prop != self.options.$prop {
										let old = own_client.$prop;
										own_client.$prop = self.options.$prop;
										events.push(events::Event::PropertyChanged {
											id: events::PropertyId::$event(own_client.id),
											old: events::PropertyValue::$event_value(old),
											invoker: None,
											extra: Default::default(),
										});
									}
								}
							};
						}

						match arg.name() {
							b"client_input_muted" => {
								set_prop!(input_muted, ClientInputMuted, Bool);
							}
							b"client_output_muted" => {
								set_prop!(output_muted, ClientOutputMuted, Bool);
							}
							b"client_away" => {
								if arg.value().get_raw() == b"1" {
									if !has_away_message {
										self.options.away = Some("".into());
									}
								} else {
									self.options.away = None;
								}
								has_is_away = true;
							}
							b"client_away_message" => {
								if has_is_away && self.options.away.is_some() {
									match arg.value().get_str() {
										Ok(r) => self.options.away = Some(r.to_string().into()),
										Err(error) => {
											warn!(%error, message = ?arg.value(),
												"Failed to parse sent away message");
										}
									}
									has_away_message = true;
								}
							}
							b"client_input_hardware" => {
								set_prop!(input_hardware_enabled, ClientInputHardwareEnabled, Bool);
							}
							b"client_output_hardware" => {
								set_prop!(
									output_hardware_enabled,
									ClientOutputHardwareEnabled,
									Bool
								);
							}
							_ => {}
						}
						if let Some(own_client) = &mut own_client {
							if own_client.away_message.as_deref()
								!= self.options.away.as_ref().map(|s| s.as_ref())
							{
								let old = mem::replace(
									&mut own_client.away_message,
									self.options.away.as_ref().map(|s| s.to_string()),
								);
								events.push(events::Event::PropertyChanged {
									id: events::PropertyId::ClientAwayMessage(own_client.id),
									old: events::PropertyValue::OptionString(old),
									invoker: None,
									extra: Default::default(),
								});
							}
						}
					}
				}

				if !events.is_empty() {
					self.stream_items.push_back(Ok(StreamItem::BookEvents(events)));
				}

				let new_can_send = Self::intern_can_send_audio(book, &self.options);
				let new_can_receive = Self::intern_can_receive_audio(book, &self.options);
				if new_can_send != prev_can_send {
					self.stream_items.push_back(Ok(StreamItem::AudioChange(
						AudioEvent::CanSendAudio(new_can_send),
					)));
				}
				if new_can_receive != prev_can_receive {
					self.stream_items.push_back(Ok(StreamItem::AudioChange(
						AudioEvent::CanReceiveAudio(new_can_receive),
					)));
				}
			}
		}
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
	/// # use tsclientlib::{Connection, DisconnectOptions, StreamItem};
	///
	/// # #[tokio::main]
	/// # async fn main() {
	/// let mut con = Connection::build("localhost").connect().unwrap();
	/// // Wait until connected
	/// con.events()
	///     // We are connected when we receive the first BookEvents
	///     .try_filter(|e| future::ready(matches!(e, StreamItem::BookEvents(_))))
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
	/// # use tsclientlib::{Connection, DisconnectOptions, Reason, StreamItem};
	///
	/// # #[tokio::main]
	/// # async fn main() {
	/// let mut con = Connection::build("localhost").connect().unwrap();
	/// // Wait until connected
	/// con.events()
	///     // We are connected when we receive the first BookEvents
	///     .try_filter(|e| future::ready(matches!(e, StreamItem::BookEvents(_))))
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

	/// Send audio to the server.
	///
	/// This function does only accept `Voice` and `VoiceWhisper` packets. Commands should be send
	/// with [`command.send(&mut connection)`](OutCommandExt::send) or
	/// [`command.send_with_result(&mut connection)`](OutCommandExt::send_with_result).
	///
	/// # Examples
	///
	/// ```no_run
	/// # let con: tsclientlib::Connection = panic!();
	/// use tsproto_packets::packets::{AudioData, CodecType, OutAudio};
	/// let codec = CodecType::OpusVoice;
	/// // Send an empty packet to signal audio end
	/// let packet = OutAudio::new(&AudioData::C2S { id: 0, codec, data: &[] });
	/// con.send_audio(packet).unwrap();
	/// ```
	pub fn send_audio(&mut self, packet: OutPacket) -> Result<()> {
		assert!(
			[PacketType::Voice, PacketType::VoiceWhisper].contains(&packet.header().packet_type()),
			"Can only send audio packets with send_audio"
		);
		if let ConnectionState::Connected { con, book } = &mut self.state {
			if !Self::intern_can_send_audio(book, &self.options) {
				let span = self.span.clone();
				warn!(parent: span, "Sending audio while muted");
			}
			con.client.send_packet(packet).map_err(Error::SendPacket)?;
			Ok(())
		} else {
			Err(Error::NotConnected)
		}
	}

	/// Download a file from a channel of the connected TeamSpeak server.
	///
	/// Returns the size of the file and a tcp stream of the requested file.
	///
	/// # Examples
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
	) -> Result<FiletransferHandle> {
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
	/// # Examples
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
	) -> Result<FiletransferHandle> {
		if let ConnectionState::Connected { con, .. } = &mut self.state {
			con.upload_file(channel_id, path, channel_password, size, overwrite, resume)
		} else {
			Err(Error::NotConnected)
		}
	}

	/// Get statistics about the network connection.
	///
	/// # Example
	/// Get the fraction of packets that are currently lost.
	///
	/// ```no_run
	/// # let con: tsclientlib::Connection = panic!();
	/// let loss: f32 = con.get_network_stats().unwrap().get_packetloss();
	/// ```
	///
	/// # Error
	/// Returns an error if currently not connected.
	pub fn get_network_stats(&self) -> Result<&ConnectionStats> {
		if let ConnectionState::Connected { con, .. } = &self.state {
			Ok(&con.client.resender.stats)
		} else {
			Err(Error::NotConnected)
		}
	}

	/// If this client can send audio or is muted.
	///
	/// If the client is muted, the server will drop any sent audio packets. A client counts as
	/// muted if input or output is muted, he is marked away or the talk power is less than the
	/// needed talk power in the current channel. Temporary disconnects also count as mutes.
	///
	/// If this changes, the [`AudioEvent::CanSendAudio`] event is emitted.
	pub fn can_send_audio(&self) -> bool {
		if let ConnectionState::Connected { book, .. } = &self.state {
			Self::intern_can_send_audio(book, &self.options)
		} else {
			false
		}
	}

	fn intern_can_send_audio(book: &data::Connection, options: &ConnectOptions) -> bool {
		if let Some(own_client) = book.clients.get(&book.own_client) {
			if own_client.input_muted
				|| !own_client.input_hardware_enabled
				|| own_client.away_message.is_some()
				|| own_client.output_muted
				|| !own_client.output_hardware_enabled
			{
				return false;
			}
			if let Some(own_channel) = book.channels.get(&own_client.channel) {
				if let Some(needed_talk_power) = own_channel.needed_talk_power {
					return own_client.talk_power_granted
						|| own_client.talk_power >= needed_talk_power;
				}
			}
			true
		} else {
			!options.input_muted
				&& options.input_hardware_enabled
				&& options.away.is_none()
				&& !options.output_muted
				&& options.output_hardware_enabled
		}
	}

	/// If this client can receive audio or the output is muted.
	///
	/// This will return `false` if the client is currently not connected to the server.
	/// Audio packets might still be received but should not be played.
	///
	/// If this changes, the [`AudioEvent::CanReceiveAudio`] event is emitted.
	pub fn can_receive_audio(&self) -> bool {
		if let ConnectionState::Connected { book, .. } = &self.state {
			Self::intern_can_receive_audio(book, &self.options)
		} else {
			false
		}
	}

	fn intern_can_receive_audio(book: &data::Connection, options: &ConnectOptions) -> bool {
		if let Some(own_client) = book.clients.get(&book.own_client) {
			!own_client.output_muted
				&& !own_client.output_only_muted
				&& own_client.output_hardware_enabled
		} else {
			!options.output_muted && options.output_hardware_enabled
		}
	}

	/// If the current channel (and server) allow unencrypted voice packets.
	///
	/// For whisper packets, only the server setting is checked.
	#[cfg(feature = "audio")]
	fn intern_can_receive_unencrypted_audio(book: &data::Connection, whisper: bool) -> bool {
		match book.server.codec_encryption_mode {
			CodecEncryptionMode::ForcedOn => false,
			CodecEncryptionMode::ForcedOff => true,
			CodecEncryptionMode::PerChannel => {
				if whisper {
					return true;
				}
				if let Some(own_client) = book.clients.get(&book.own_client) {
					if let Some(own_channel) = book.channels.get(&own_client.channel) {
						return own_channel.is_unencrypted.unwrap_or(true);
					}
				}
				true
			}
		}
	}

	fn poll_next(&mut self, cx: &mut Context) -> Poll<Option<Result<StreamItem>>> {
		let _span = self.span.clone().entered();
		if let Some(item) = self.stream_items.pop_front() {
			return Poll::Ready(Some(item));
		}
		match &mut self.state {
			ConnectionState::Connecting(fut, reconnect) => match fut.poll_unpin(cx) {
				Poll::Pending => Poll::Pending,
				Poll::Ready(Err(Error::IdentityLevel(level))) => {
					if let Err(e) = self.increase_identity_level(level) {
						return Poll::Ready(Some(Err(e)));
					}
					Poll::Ready(Some(Ok(StreamItem::IdentityLevelIncreasing(level))))
				}
				Poll::Ready(Err(e)) => {
					if *reconnect {
						if let Error::ConnectFailed { errors, .. } = &e {
							for e in errors {
								if let Error::Connect(client::Error::TsProto(
									tsproto::Error::Timeout(reason),
								)) = e
								{
									debug!(timeout = reason, "Connect failed, reconnecting");
									let fut =
										Self::connect(self.options.clone(), true).in_current_span();
									self.state = ConnectionState::Connecting(Box::pin(fut), true);
									return self.poll_next(cx);
								}
							}
						}
					}
					Poll::Ready(Some(Err(e)))
				}
				Poll::Ready(Ok((client, book))) => {
					let con = ConnectedConnection {
						client,
						cur_return_code: 0,
						cur_filetransfer_id: 0,
						subscribed: false,
						filetransfers: Default::default(),
						connection_time: OffsetDateTime::now_utc(),
					};
					if Self::intern_can_send_audio(&book, &self.options) {
						self.stream_items
							.push_back(Ok(StreamItem::AudioChange(AudioEvent::CanSendAudio(true))));
					}
					if Self::intern_can_receive_audio(&book, &self.options) {
						self.stream_items.push_back(Ok(StreamItem::AudioChange(
							AudioEvent::CanReceiveAudio(true),
						)));
					}
					self.state = ConnectionState::Connected { con, book };
					Poll::Ready(Some(Ok(StreamItem::BookEvents(vec![
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
				Poll::Ready(Ok(identity)) => {
					self.options.identity = Some(identity);
					let fut = Self::connect(self.options.clone(), false);
					self.state =
						ConnectionState::Connecting(Box::pin(fut.in_current_span()), false);
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

						if let client::Error::TsProto(tsproto::Error::Timeout(reason)) = e {
							// Reconnect on timeout
							warn!(timeout = reason, "Connection failed, reconnecting");
							let fut = Self::connect(self.options.clone(), true);
							self.state =
								ConnectionState::Connecting(Box::pin(fut.in_current_span()), true);
							return Poll::Ready(Some(Ok(StreamItem::DisconnectedTemporarily(
								TemporaryDisconnectReason::Timeout(reason),
							))));
						} else {
							break Poll::Ready(Some(Err(Error::ConnectionFailed(e))));
						}
					}
					Poll::Ready(Some(Ok(item))) => match item {
						ProtoStreamItem::Error(error) => {
							warn!(%error, "Connection got a non-fatal error");
						}
						#[cfg(feature = "audio")]
						ProtoStreamItem::Audio(audio) => {
							if !Self::intern_can_receive_audio(book, &self.options) {
								info!("Received audio packet while output is muted, ignoring");
							} else {
								let encrypted = !audio
									.data()
									.packet()
									.header()
									.flags()
									.contains(tsproto_packets::packets::Flags::UNENCRYPTED);
								let whisper = audio.data().packet().header().packet_type()
									== PacketType::VoiceWhisper;
								if encrypted
									|| Self::intern_can_receive_unencrypted_audio(book, whisper)
								{
									return Poll::Ready(Some(Ok(StreamItem::Audio(audio))));
								} else {
									info!(
										"Received unencrypted audio packet but only encrypted \
										 packets are allowed, ignoring"
									);
								}
							}
						}
						ProtoStreamItem::Command(cmd) => {
							let prev_can_send = Self::intern_can_send_audio(book, &self.options);
							let prev_can_receive =
								Self::intern_can_receive_audio(book, &self.options);
							if let Some(reason) = con.handle_command(
								book,
								&mut self.stream_items,
								&mut self.options,
								cmd,
							) {
								warn!("Server shut down, reconnecting");
								let fut = Self::connect(self.options.clone(), true);
								self.state = ConnectionState::Connecting(
									Box::pin(fut.in_current_span()),
									true,
								);
								if prev_can_send {
									self.stream_items.push_back(Ok(StreamItem::AudioChange(
										AudioEvent::CanSendAudio(false),
									)));
								}
								if prev_can_receive {
									self.stream_items.push_back(Ok(StreamItem::AudioChange(
										AudioEvent::CanReceiveAudio(false),
									)));
								}
								self.stream_items
									.push_back(Ok(StreamItem::DisconnectedTemporarily(reason)));
								return Poll::Ready(Some(self.stream_items.pop_front().unwrap()));
							}

							let new_can_send = Self::intern_can_send_audio(book, &self.options);
							let new_can_receive =
								Self::intern_can_receive_audio(book, &self.options);
							if new_can_send != prev_can_send {
								self.stream_items.push_back(Ok(StreamItem::AudioChange(
									AudioEvent::CanSendAudio(new_can_send),
								)));
							}
							if new_can_receive != prev_can_receive {
								self.stream_items.push_back(Ok(StreamItem::AudioChange(
									AudioEvent::CanReceiveAudio(new_can_receive),
								)));
							}
							if let Some(item) = self.stream_items.pop_front() {
								break Poll::Ready(Some(item));
							}
						}
						ProtoStreamItem::NetworkStatsUpdated => {
							return Poll::Ready(Some(Ok(StreamItem::NetworkStatsUpdated)));
						}
						_ => {}
					},
				}
			} {
				Poll::Ready(r) => Poll::Ready(r),
				Poll::Pending => {
					// Check file transfers
					let ft = con.filetransfers.iter_mut().enumerate().find_map(|(i, ft)| match ft
						.poll_unpin(cx)
					{
						Poll::Pending => None,
						Poll::Ready(r) => Some((i, r)),
					});
					if let Some((i, res)) = ft {
						let _ = con.filetransfers.remove(i);
						Poll::Ready(Some(Ok(res)))
					} else {
						Poll::Pending
					}
				}
			},
		}
	}
}

impl<T: OutMessageTrait> OutCommandExt for T {
	fn send_with_result(self, con: &mut Connection) -> Result<MessageHandle> {
		con.send_command_with_result(self.to_packet())
	}

	fn send(self, con: &mut Connection) -> Result<()> { con.send_command(self.to_packet()) }
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
		&mut self, book: &mut data::Connection, stream_items: &mut VecDeque<Result<StreamItem>>,
		options: &mut ConnectOptions, cmd: InCommandBuf,
	) -> Option<TemporaryDisconnectReason> {
		let msg = match InMessage::new(cmd.data().packet().header(), cmd.data().packet().content())
		{
			Ok(r) => r,
			Err(error) => {
				warn!(%error, "Failed to parse message");
				return None;
			}
		};

		let mut handled = true;
		if let InMessage::CommandError(msg) = &msg {
			// Handle error messages
			for msg in msg.iter() {
				if let Some(ret_code) = msg.return_code.as_ref().and_then(|r| r.parse().ok()) {
					let res = if msg.id == TsError::Ok {
						Ok(())
					} else {
						Err(CommandError {
							error: msg.id,
							missing_permission: msg.missing_permission_id,
						})
					};
					stream_items
						.push_back(Ok(StreamItem::MessageResult(MessageHandle(ret_code), res)));
				} else {
					handled = false;
				}
			}
		} else if let InMessage::FileDownload(msg) = &msg {
			for msg in msg.iter() {
				let ft_id = FiletransferHandle(msg.client_filetransfer_id);
				let ip = msg.ip.unwrap_or_else(|| self.client.address.ip());
				let addr = SocketAddr::new(ip, msg.port);
				let key = msg.filetransfer_key.clone();
				let size = msg.size;

				let fut = Box::new(async move {
					let addr = addr;
					let key = key;
					let mut stream =
						TcpStream::connect(&addr).await.map_err(Error::FiletransferIo)?;
					stream.write_all(key.as_bytes()).await.map_err(Error::FiletransferIo)?;
					stream.flush().await.map_err(Error::FiletransferIo)?;
					Ok(stream)
				})
				.map(move |res| match res {
					Ok(stream) => {
						StreamItem::FileDownload(ft_id, FileDownloadResult { size, stream })
					}
					Err(e) => StreamItem::FiletransferFailed(ft_id, e),
				});

				self.filetransfers.push(Box::pin(fut));
			}
		} else if let InMessage::FileUpload(msg) = &msg {
			for msg in msg.iter() {
				let ft_id = FiletransferHandle(msg.client_filetransfer_id);
				let ip = msg.ip.unwrap_or_else(|| self.client.address.ip());
				let addr = SocketAddr::new(ip, msg.port);
				let key = msg.filetransfer_key.clone();
				let seek_position = msg.seek_position;

				let fut = Box::new(async move {
					let addr = addr;
					let key = key;
					let mut stream =
						TcpStream::connect(&addr).await.map_err(Error::FiletransferIo)?;
					stream.write_all(key.as_bytes()).await.map_err(Error::FiletransferIo)?;
					stream.flush().await.map_err(Error::FiletransferIo)?;
					Ok(stream)
				})
				.map(move |res| match res {
					Ok(stream) => {
						StreamItem::FileUpload(ft_id, FileUploadResult { seek_position, stream })
					}
					Err(e) => StreamItem::FiletransferFailed(ft_id, e),
				});

				self.filetransfers.push(Box::pin(fut));
			}
		} else if let InMessage::FiletransferStatus(msg) = &msg {
			for msg in msg.iter() {
				let ft_id = FiletransferHandle(msg.client_filetransfer_id);
				let err = CommandError { error: msg.status, missing_permission: None };
				stream_items.push_back(Ok(StreamItem::FiletransferFailed(ft_id, err.into())));
			}
		} else if let InMessage::ClientConnectionInfoUpdateRequest(_) = &msg {
			// Answer with connection stats
			let stats = &self.client.resender.stats;
			let last_second_bytes = stats.get_last_second_bytes();
			let last_minute_bytes = stats.get_last_minute_bytes();
			let packet = c2s::OutSetConnectionInfoMessage::new(&mut iter::once(
				c2s::OutSetConnectionInfoPart {
					ping: stats.rtt.try_into().unwrap_or_else(|_| time::Duration::seconds(1)),
					ping_deviation: stats
						.rtt_dev
						.try_into()
						.unwrap_or_else(|_| time::Duration::seconds(1)),
					packets_sent_speech: stats.total_packets[PacketStat::OutSpeech as usize],
					packets_sent_keepalive: stats.total_packets[PacketStat::OutKeepalive as usize],
					packets_sent_control: stats.total_packets[PacketStat::OutControl as usize],
					bytes_sent_speech: stats.total_bytes[PacketStat::OutSpeech as usize],
					bytes_sent_keepalive: stats.total_bytes[PacketStat::OutKeepalive as usize],
					bytes_sent_control: stats.total_bytes[PacketStat::OutControl as usize],
					packets_received_speech: stats.total_packets[PacketStat::InSpeech as usize],
					packets_received_keepalive: stats.total_packets
						[PacketStat::InKeepalive as usize],
					packets_received_control: stats.total_packets[PacketStat::InControl as usize],
					bytes_received_speech: stats.total_bytes[PacketStat::InSpeech as usize],
					bytes_received_keepalive: stats.total_bytes[PacketStat::InKeepalive as usize],
					bytes_received_control: stats.total_bytes[PacketStat::InControl as usize],
					server_to_client_packetloss_speech: stats.get_packetloss_s2c_speech(),
					server_to_client_packetloss_keepalive: stats.get_packetloss_s2c_keepalive(),
					server_to_client_packetloss_control: stats.get_packetloss_s2c_control(),
					server_to_client_packetloss_total: stats.get_packetloss_s2c_total(),
					bandwidth_sent_last_second_speech: last_second_bytes
						[PacketStat::OutSpeech as usize]
						as u64,
					bandwidth_sent_last_second_keepalive: last_second_bytes
						[PacketStat::OutKeepalive as usize]
						as u64,
					bandwidth_sent_last_second_control: last_second_bytes
						[PacketStat::OutControl as usize]
						as u64,
					bandwidth_sent_last_minute_speech: last_minute_bytes
						[PacketStat::OutSpeech as usize]
						/ 60,
					bandwidth_sent_last_minute_keepalive: last_minute_bytes
						[PacketStat::OutKeepalive as usize]
						/ 60,
					bandwidth_sent_last_minute_control: last_minute_bytes
						[PacketStat::OutControl as usize]
						/ 60,
					bandwidth_received_last_second_speech: last_second_bytes
						[PacketStat::InSpeech as usize]
						as u64,
					bandwidth_received_last_second_keepalive: last_second_bytes
						[PacketStat::InKeepalive as usize]
						as u64,
					bandwidth_received_last_second_control: last_second_bytes
						[PacketStat::InControl as usize]
						as u64,
					bandwidth_received_last_minute_speech: last_minute_bytes
						[PacketStat::InSpeech as usize]
						/ 60,
					bandwidth_received_last_minute_keepalive: last_minute_bytes
						[PacketStat::InKeepalive as usize]
						/ 60,
					bandwidth_received_last_minute_control: last_minute_bytes
						[PacketStat::InControl as usize]
						/ 60,
				},
			));

			if let Err(e) = self.client.send_packet(packet.into_packet()) {
				stream_items.push_back(Err(Error::SendPacket(e)));
			}
		} else {
			let (mut events, book_handled) = match book.handle_command(&msg) {
				Ok(r) => r,
				Err(error) => {
					warn!(%error, "Failed to handle message");
					(Vec::new(), false)
				}
			};

			if let InMessage::ClientConnectionInfo(msg) = &msg {
				for msg in msg.iter() {
					if msg.client_id == book.own_client && msg.connected_time.is_none() {
						if let Some(own_client) = book.clients.get_mut(&book.own_client) {
							if let Some(connection_data) = &mut own_client.connection_data {
								connection_data.connected_time =
									Some(OffsetDateTime::now_utc() - self.connection_time);
							}
						}
					}
				}
			}

			handled = book_handled;
			self.client.hand_back_buffer(cmd.into_buffer());
			if !events.is_empty() {
				// Ignore mute state update for own client
				let own_id = book.own_client;
				if let Some(own_client) = book.clients.get_mut(&own_id) {
					events.retain(|e| {
						if let events::Event::PropertyChanged { id, old, .. } = e {
							macro_rules! reset {
								($msg:ident, $obj:ident) => {
									if *id == events::PropertyId::$msg(own_id) {
										if let events::PropertyValue::Bool(b) = old {
											own_client.$obj = *b;
										}
										return false;
									}
								};
							}

							reset!(ClientInputMuted, input_muted);
							reset!(ClientOutputMuted, output_muted);
							reset!(ClientInputHardwareEnabled, input_hardware_enabled);
							reset!(ClientOutputHardwareEnabled, output_hardware_enabled);
							reset!(ClientOutputOnlyMuted, output_only_muted);

							if *id == events::PropertyId::ClientAwayMessage(own_id) {
								if let events::PropertyValue::OptionString(s) = old {
									own_client.away_message = s.clone();
								}
								return false;
							}
						}
						true
					});
				}

				stream_items.push_back(Ok(StreamItem::BookEvents(events)));
			}
		}

		if let InMessage::ChannelCreated(msg) = &msg {
			// Handle subscriptions
			// - If the client (server groups or permissions) change, he stays subscribed (even if he
			//   does not have the power anymore).
			// - If the channel is edited, the subscription status gets updated.
			// - If we enter a channel, we get subscribed but there is no notification that we are
			//   subscribed.
			// - If we leave a channel, there is a notification that we unsubscribed.
			// - If a new channel is created, we are not automatically subscribed.
			if self.subscribed {
				// Subscribe to new channels
				let packet = c2s::OutChannelSubscribeMessage::new(
					&mut msg
						.iter()
						.map(|msg| c2s::OutChannelSubscribePart { channel_id: msg.channel_id }),
				);
				if let Err(error) = self.client.send_packet(packet.into_packet()) {
					warn!(%error, "Failed to send channel subscribe packet");
				}
			}
		} else if let InMessage::ClientLeftView(msg) = &msg {
			// Handle server restarts
			for msg in msg.iter() {
				if msg.client_id == book.own_client
					&& matches!(
						msg.reason,
						Some(Reason::Serverstop) | Some(Reason::ClientdisconnectServerShutdown)
					) {
					return Some(TemporaryDisconnectReason::Serverstop);
				}
			}
		} else if let InMessage::ClientMoved(msg) = &msg {
			// Save channel switches in options
			for msg in msg.iter() {
				if msg.client_id == book.own_client && msg.target_channel_id.0 != 0 {
					options.channel = Some(format!("/{}", msg.target_channel_id.0).into());
					break;
				}
			}
		} else if let InMessage::ClientUpdated(msg) = &msg {
			// Save name change in options
			for msg in msg.iter() {
				if msg.client_id == book.own_client {
					if let Some(name) = &msg.name {
						options.name = name.to_string().into();
					}
				}
			}
		}

		if !handled {
			stream_items.push_back(Ok(StreamItem::MessageEvent(msg)));
		}
		None
	}

	fn send_command_with_result(&mut self, mut packet: OutCommand) -> Result<MessageHandle> {
		let code = self.cur_return_code;
		self.cur_return_code += 1;
		packet.write_arg("return_code", &code);

		self.send_command(packet).map(|_| MessageHandle(code))
	}

	fn send_command(&mut self, packet: OutCommand) -> Result<()> {
		self.client.send_packet(packet.into_packet()).map(|_| ()).map_err(Error::SendPacket)
	}

	fn download_file(
		&mut self, channel_id: ChannelId, path: &str, channel_password: Option<&str>,
		seek_position: Option<u64>,
	) -> Result<FiletransferHandle> {
		let ft_id = self.cur_filetransfer_id;
		self.cur_filetransfer_id += 1;
		let pass = channel_password
			.map(|p| tsproto_types::crypto::encode_password(p.as_bytes()))
			.unwrap_or_default();
		let packet = c2s::OutInitDownloadMessage::new(&mut iter::once(c2s::OutInitDownloadPart {
			client_filetransfer_id: ft_id,
			name: Cow::Borrowed(path),
			channel_id,
			channel_password: Cow::Borrowed(&pass),
			seek_position: seek_position.unwrap_or_default(),
			protocol: 1,
		}));

		self.send_command_with_result(packet).map(|_| FiletransferHandle(ft_id))
	}

	fn upload_file(
		&mut self, channel_id: ChannelId, path: &str, channel_password: Option<&str>, size: u64,
		overwrite: bool, resume: bool,
	) -> Result<FiletransferHandle> {
		let ft_id = self.cur_filetransfer_id;
		self.cur_filetransfer_id += 1;

		let pass = channel_password
			.map(|p| tsproto_types::crypto::encode_password(p.as_bytes()))
			.unwrap_or_default();
		let packet = c2s::OutInitUploadMessage::new(&mut iter::once(c2s::OutInitUploadPart {
			client_filetransfer_id: ft_id,
			name: Cow::Borrowed(path),
			channel_id,
			channel_password: Cow::Borrowed(&pass),
			overwrite,
			resume,
			size,
			protocol: 1,
		}));

		self.send_command_with_result(packet).map(|_| FiletransferHandle(ft_id))
	}
}

/// The configuration for creating a new connection.
///
/// # Example
///
/// ```
/// # use tsclientlib::Connection;
/// let config = Connection::build("localhost")
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
	server: Option<UidBuf>,
	name: Cow<'static, str>,
	version: Version,
	hardware_id: Cow<'static, str>,
	channel: Option<Cow<'static, str>>,
	channel_password: Option<Cow<'static, str>>,
	password: Option<Cow<'static, str>>,
	input_muted: bool,
	output_muted: bool,
	input_hardware_enabled: bool,
	output_hardware_enabled: bool,
	away: Option<Cow<'static, str>>,
	log_commands: bool,
	log_packets: bool,
	log_udp_packets: bool,
}

impl ConnectOptions {
	/// Start creating the configuration of a new connection.
	///
	/// # Arguments
	/// The address of the server has to be supplied. The address can be a
	/// [`SocketAddr`](std::net::SocketAddr), a string or directly a [`ServerAddress`]. A string
	/// will automatically be resolved from all formats supported by TeamSpeak.
	/// For details, see [`resolver::resolve`].
	#[inline]
	// TODO Remove
	#[deprecated(since = "0.2.0", note = "Connection::build should be used instead")]
	pub fn new<A: Into<ServerAddress>>(address: A) -> Self { Connection::build(address) }

	/// Create the connection object from these options.
	///
	/// This function opens a new connection to a server. The connection needs to be polled for
	/// events to do something.
	///
	/// # Examples
	/// This will open a connection to the TeamSpeak server at `localhost`.
	///
	/// ```no_run
	/// # use futures::prelude::*;
	/// # use tsclientlib::{Connection, StreamItem};
	///
	/// # #[tokio::main]
	/// # async fn main() {
	///     let mut con = Connection::build("localhost").connect().unwrap();
	///     // Wait until connected
	///     con.events()
	///         // We are connected when we receive the first BookEvents
	///         .try_filter(|e| future::ready(matches!(e, StreamItem::BookEvents(_))))
	///         .next()
	///         .await
	///         .unwrap();
	/// # }
	/// ```
	pub fn connect(mut self) -> Result<Connection> {
		let span = info_span!("connection", addr = %self.address).entered();

		#[cfg(debug_assertions)]
		let profile = "Debug";
		#[cfg(not(debug_assertions))]
		let profile = "Release";

		info!(
			version = git_testament::render_testament!(TESTAMENT).as_str(),
			profile,
			tsproto_version = git_testament::render_testament!(tsproto::get_testament()).as_str(),
			"tsclientlib"
		);

		let mut stream_items = VecDeque::new();
		self.identity = Some(self.identity.take().unwrap_or_else(|| {
			// Create new ECDH key
			let id = Identity::create();
			// Send event
			stream_items.push_back(Ok(StreamItem::IdentityLevelIncreased));
			id
		}));

		// Try all addresses
		let fut = Connection::connect(self.clone(), false);

		Ok(Connection {
			state: ConnectionState::Connecting(Box::pin(fut.in_current_span()), false),
			span: span.exit(),
			options: self,
			stream_items,
		})
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

	/// Check the identity of the server to be equal with the given uid.
	///
	/// If the public key of the server does not match this uid, the connection will be aborted.
	///
	/// # Default
	/// Allow the server to have any identity.
	#[inline]
	pub fn server(mut self, server: UidBuf) -> Self {
		self.server = Some(server);
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
	/// # use tsclientlib::Connection;
	/// let opts = Connection::build("localhost").channel("Default Channel");
	/// ```
	///
	/// Connecting to a channel further down in the hierarchy.
	/// ```
	/// # use tsclientlib::Connection;
	/// let opts = Connection::build("localhost").channel("Default Channel/Nested");
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
	/// # use tsclientlib::{ChannelId, Connection};
	/// let opts = Connection::build("localhost").channel_id(ChannelId(2));
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
	/// # use tsclientlib::Connection;
	/// let opts = Connection::build("localhost")
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
	/// # use tsclientlib::Connection;
	/// let opts = Connection::build("localhost").password("My secret password");
	/// ```
	#[inline]
	pub fn password<S: Into<Cow<'static, str>>>(mut self, pwd: S) -> Self {
		self.password = Some(pwd.into());
		self
	}

	/// Connect to the server in a muted state.
	///
	/// # Example
	/// ```
	/// # use tsclientlib::Connection;
	/// let opts = Connection::build("localhost").input_muted(true);
	/// ```
	#[inline]
	pub fn input_muted(mut self, input_muted: bool) -> Self {
		self.input_muted = input_muted;
		self
	}

	/// Connect to the server with the output muted.
	///
	/// # Example
	/// ```
	/// # use tsclientlib::Connection;
	/// let opts = Connection::build("localhost").output_muted(true);
	/// ```
	#[inline]
	pub fn output_muted(mut self, output_muted: bool) -> Self {
		self.output_muted = output_muted;
		self
	}

	/// Connect to the server with input hardware disabled state.
	///
	/// # Example
	/// ```
	/// # use tsclientlib::Connection;
	/// let opts = Connection::build("localhost").input_hardware_enabled(true);
	/// ```
	#[inline]
	pub fn input_hardware_enabled(mut self, input_hardware_enabled: bool) -> Self {
		self.input_hardware_enabled = input_hardware_enabled;
		self
	}

	/// Connect to the server with output hardware disabled stat.
	///
	/// # Example
	/// ```
	/// # use tsclientlib::Connection;
	/// let opts = Connection::build("localhost").output_hardware_enabled(true);
	/// ```
	#[inline]
	pub fn output_hardware_enabled(mut self, output_hardware_enabled: bool) -> Self {
		self.output_hardware_enabled = output_hardware_enabled;
		self
	}

	/// Connect to the server with an away message.
	///
	/// # Example
	/// ```
	/// # use tsclientlib::Connection;
	/// let opts = Connection::build("localhost").away("Buying groceries");
	/// ```
	#[inline]
	pub fn away<S: Into<Cow<'static, str>>>(mut self, message: S) -> Self {
		self.away = Some(message.into());
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

	#[inline]
	pub fn get_address(&self) -> &ServerAddress { &self.address }
	#[inline]
	pub fn get_local_address(&self) -> Option<&SocketAddr> { self.local_address.as_ref() }
	#[inline]
	pub fn get_identity(&self) -> Option<&Identity> { self.identity.as_ref() }
	#[inline]
	pub fn get_server(&self) -> Option<&Uid> { self.server.as_deref() }
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
	pub fn get_input_muted(&self) -> bool { self.input_muted }
	#[inline]
	pub fn get_output_muted(&self) -> bool { self.output_muted }
	#[inline]
	pub fn get_input_hardware_enabled(&self) -> bool { self.input_hardware_enabled }
	#[inline]
	pub fn get_output_hardware_enabled(&self) -> bool { self.output_hardware_enabled }
	#[inline]
	pub fn get_away(&self) -> Option<&str> { self.away.as_ref().map(AsRef::as_ref) }
	#[inline]
	pub fn get_log_commands(&self) -> bool { self.log_commands }
	#[inline]
	pub fn get_log_packets(&self) -> bool { self.log_packets }
	#[inline]
	pub fn get_log_udp_packets(&self) -> bool { self.log_udp_packets }
}
