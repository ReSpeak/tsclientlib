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

use std::collections::VecDeque;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::{iter, result};

use anyhow::{bail, format_err, Error, Result};
use futures::prelude::*;
use slog::{debug, info, o, warn, Drain, Logger};
use thiserror::Error;
use tokio::io::AsyncWriteExt as _;
use tokio::net::{TcpStream, UdpSocket};
use tokio::sync::oneshot;
use ts_bookkeeping::messages::c2s;
use ts_bookkeeping::messages::s2c::InMessage;
use tsproto::client;
use tsproto::connection::StreamItem as ProtoStreamItem;
use tsproto_packets::packets::{InCommandBuf, OutCommand};

mod facades;
pub mod resolver;

// The build environment of tsclientlib.
git_testament::git_testament!(TESTAMENT);

#[cfg(test)]
mod tests;

// Reexports
pub use ts_bookkeeping::*;
pub use tsproto::Identity;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct MessageHandle(pub u16);
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct FileTransferHandle(pub u16);

#[derive(Debug)]
pub struct FileDownloadResult {
	/// The size of the requested file.
	// TODO The rest of the size when a seek_position is specified?
	pub size: u64,
	/// The stream where the file can be downloaded.
	pub stream: TcpStream,
}

#[derive(Debug)]
pub struct FileUploadResult {
	/// The size of the already uploaded part when `resume` was set to `true`
	/// in [`download_file`].
	///
	/// [`download_file`]: struct.Connection.html#method.download_file
	// TODO Link works?
	pub seek_position: u64,
	/// The stream where the file can be uploaded.
	pub stream: TcpStream,
}

/// An event that gets returned by the connection.
///
/// A stream of these events is returned by [`Connection::events`].
///
/// [`Connection::events`]: struct.Connection.html#method.events
// TODO Link works?
#[derive(Debug)]
pub enum StreamItem {
	/// All the incoming events.
	///
	/// If a connection to the server was established this will contain an added
	/// event of a server.
	ConEvents(Vec<events::Event>),
	/// The needed level.
	IdentityLevelIncreasing(u8),
	/// This event may occur without an `IdentityLevelIncreasing` event before
	/// if a new identity is created because no identity was supplied.
	IdentityLevelIncreased,
	/// The connection timed out or the server shut down. The connection will be
	/// rebuilt automatically.
	DisconnectedTemporarily,
	/// The result of sending a message.
	MessageResult(MessageHandle, result::Result<(), TsError>),
	FileDownload(FileTransferHandle, FileDownloadResult),
	FileUpload(FileTransferHandle, FileUploadResult),
	FileTransferFailed(FileTransferHandle, Error),
}

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
	Connecting(
		future::BoxFuture<
			'static,
			result::Result<(client::Client, data::Connection), ConnectError>,
		>,
	),
	IdentityLevelIncreasing {
		/// The wanted level
		level: u8,
		/// We get the improved identity here.
		recv: oneshot::Receiver<Result<Identity>>,
		state: Arc<Mutex<IdentityIncreaseLevelState>>,
	},
	Connected {
		con: ConnectedConnection,
		book: data::Connection,
	},
}

struct EventStream<'a>(&'a mut Connection);

enum IdentityIncreaseLevelState {
	Computing,
	/// Set to this state to cancel the computation.
	Canceled,
}

#[derive(Debug, Error)]
enum ConnectError {
	#[error("Need to increase the identity level to {0}")]
	IdentityLevelIncrease(u8),
	#[error("Got error {0}")]
	TsError(TsError),
	#[error(transparent)]
	Other(#[from] anyhow::Error),
}

/// The main type of this crate, which represents a connection to a server.
///
/// A new connection can be opened with the [`Connection::new`] function.
///
/// # Examples
/// This will open a connection to the TeamSpeak server at `localhost`.
///
/// ```no_run
/// use tokio::prelude::*;
/// use tsclientlib::{Connection, ConnectOptions};
///
/// #[tokio::main]
/// async fn main() {
///     Connection::new(ConnectOptions::new("localhost")).await.unwrap();
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
		options.identity =
			Some(options.identity.take().map(Ok).unwrap_or_else(|| {
				// Create new ECDH key
				let id = Identity::create();
				if id.is_ok() {
					// Send event
					stream_items
						.push_back(Ok(StreamItem::IdentityLevelIncreased));
				}
				id
			})?);

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
	/// The identity of the options can be updated while connecting when the
	/// identity level needs to be improved.
	pub fn get_options(&self) -> &ConnectOptions { &self.options }

	/// Get a stream of events. This is the main interaction point with a
	/// connection. You need to poll the event stream, otherwise nothing will
	/// happen in a connection.
	pub fn events<'a>(
		&'a mut self,
	) -> impl Stream<Item = Result<StreamItem>> + 'a {
		EventStream(self)
	}

	async fn connect(
		logger: Logger, options: ConnectOptions,
	) -> result::Result<(client::Client, data::Connection), ConnectError> {
		let resolved = match &options.address {
			ServerAddress::SocketAddr(a) => {
				stream::once(future::ok(*a)).left_stream()
			}
			ServerAddress::Other(s) => {
				resolver::resolve(logger.clone(), s.into()).right_stream()
			}
		};
		pin_utils::pin_mut!(resolved);
		let mut resolved: Pin<_> = resolved;

		while let Some(addr) = resolved.next().await {
			let addr = addr?;
			match Self::connect_to(&logger, &options, addr).await {
				Ok(res) => return Ok(res),
				Err(ConnectError::Other(e)) => {
					info!(logger, "Connecting failed, trying next address";
						"error" => %e);
				}
				Err(e) => {
					// Either increase identity level or the server refused us
					return Err(e);
				}
			}
		}
		Err(format_err!(
			"Failed to connect to server, address {:?} did not work",
			options.address
		)
		.into())
	}

	async fn connect_to(
		logger: &Logger, options: &ConnectOptions, addr: SocketAddr,
	) -> result::Result<(client::Client, data::Connection), ConnectError> {
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
			.map_err(Error::from)?,
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
		client.connect().await.map_err(Error::from)?;

		// Create clientinit packet
		let client_version = options.version.get_version_string();
		let client_platform = options.version.get_platform();
		let client_version_sign =
			base64::encode(options.version.get_signature());
		let default_channel =
			options.channel.as_ref().map(|s| s.as_str()).unwrap_or_default();
		let default_channel_password = options
			.channel_password
			.as_ref()
			.map(|s| s.as_str())
			.unwrap_or_default();
		let password =
			options.password.as_ref().map(|s| s.as_str()).unwrap_or_default();

		let packet = c2s::OutClientInitMessage::new(&mut iter::once(
			c2s::OutClientInitPart {
				name: &options.name,
				client_version: &client_version,
				client_platform: &client_platform,
				input_hardware_enabled: true,
				output_hardware_enabled: true,
				default_channel: &default_channel,
				default_channel_password: &default_channel_password,
				password: &password,
				metadata: "",
				client_version_sign: &client_version_sign,
				client_key_offset: counter,
				phonetic_name: "",
				default_token: "",
				hardware_id: "923f136fb1e22ae6ce95e60255529c00,\
				              d13231b1bc33edfecfb9169cc7a63bcc",
				badges: None,
			},
		));

		// TODO Error::from?
		client.send_packet(packet.into_packet()).map_err(Error::from)?;

		// Wait until we received the initserver packet.

		let cmd = client
			.filter_commands(|_, cmd| Ok(Some(cmd)))
			.await
			.map_err(Error::from)?;
		let msg = InMessage::new(
			logger,
			cmd.data().packet().header(),
			cmd.data().packet().content(),
		)
		.map_err(Error::from)?;
		match msg {
			InMessage::CommandError(e) => {
				let e = e.iter().next().unwrap();
				if e.id
					== ts_bookkeeping::TsError::ClientCouldNotValidateIdentity
				{
					if let Some(needed) = e
						.extra_message
						.as_ref()
						.and_then(|m| m.parse::<u8>().ok())
					{
						return Err(ConnectError::IdentityLevelIncrease(
							needed,
						));
					}
				}
				return Err(ConnectError::TsError(e.id));
			}
			InMessage::InitServer(initserver) => {
				// Get uid of server
				let uid = {
					let params = if let Some(r) = &client.params {
						r
					} else {
						return Err(format_err!(
							"We should be connected but the connection params \
							 do not exist"
						)
						.into());
					};

					params
						.public_key
						.get_uid_no_base64()
						.map_err(Error::from)?
				};

				// Create connection
				let data = data::Connection::new(Uid(uid), &initserver);

				Ok((client, data))
			}
			_ => Err(format_err!("Got no initserver but {:?}", msg).into()),
		}
	}

	/// If this returns `None`, the level was increased and we should try
	/// connecting again.
	fn increase_identity_level(&mut self, needed: u8) -> Result<()> {
		if needed > 20 {
			bail!(
				"The server needs an identity of level {}, please increase \
				 your identity level",
				needed
			);
		}

		let identity = self.options.identity.as_ref().unwrap().clone();
		let level = identity.level()?;
		if level >= needed {
			bail!(
				"The server requested an identity of level {}, but we already \
				 have level {}",
				needed,
				level
			);
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

		self.state = ConnectionState::IdentityLevelIncreasing {
			level: needed,
			recv,
			state,
		};
		Ok(())
	}

	pub fn cancel_identity_level_increase(&mut self) -> Result<()> {
		if let ConnectionState::IdentityLevelIncreasing { state, .. } =
			&mut self.state
		{
			*state.lock().unwrap() = IdentityIncreaseLevelState::Canceled;
		}
		Ok(())
	}

	/// Fails if disconnected
	#[cfg(feature = "unstable")]
	pub fn get_tsproto_client(&self) -> Result<&client::Client> {
		if let ConnectionState::Connected(c) = &self.state {
			Ok(c)
		} else {
			bail!("Not connected")
		}
	}

	/// Fails if disconnected
	#[cfg(feature = "unstable")]
	pub fn get_tsproto_client_mut(&mut self) -> Result<&mut client::Client> {
		if let ConnectionState::Connected(c) = &mut self.state {
			Ok(c)
		} else {
			bail!("Not connected")
		}
	}

	/// Returns the public key of the server, fails if disconnected.
	#[cfg(feature = "unstable")]
	pub fn get_server_key(&self) -> Result<tsproto::crypto::EccKeyPubP256> {
		self.get_tsproto_client().and_then(|c| {
			if let Some(params) = &c.params {
				Ok(params.public_key.clone())
			} else {
				bail!("Connection is not connected")
			}
		})
	}

	/// Adds a `return_code` to the command and returns if the corresponding
	/// answer is received. If an error occurs, the future will return an error.
	#[cfg(feature = "unstable")]
	pub fn send_command(&self, packet: OutCommand) -> Result<MessageHandle> {
		if let ConnectionState::Connected { con, .. } = &mut self.state {
			con.send_command(packet)
		} else {
			bail!("Currently not connected");
		}
	}

	pub fn get_state(&self) -> Result<&data::Connection> {
		if let ConnectionState::Connected { book, .. } = &self.state {
			Ok(book)
		} else {
			bail!("Currently not connected");
		}
	}

	pub fn get_state_mut(&mut self) -> Result<facades::ConnectionMut> {
		if let ConnectionState::Connected { con, book } = &mut self.state {
			Ok(facades::ConnectionMut { connection: con, inner: book })
		} else {
			bail!("Currently not connected");
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
	#[must_use = "futures do nothing unless polled"]
	pub async fn disconnect(&mut self, options: DisconnectOptions) {
		if let ConnectionState::Connected { con, book } = &mut self.state {
			let packet = book.disconnect(options);
			if let Err(e) = con.client.send_packet(packet.into_packet()) {
				warn!(self.logger, "Failed to send disconnect packet";
					"error" => ?e);
				return;
			}
			if let Err(e) = con.client.wait_disconnect().await {
				warn!(self.logger, "Error when disconnecting";
					"error" => ?e);
			}
		}
	}

	/// Return the size of the file and a tcp stream of the requested file.
	pub fn download_file(
		&mut self, channel_id: ChannelId, path: &str,
		channel_password: Option<&str>, seek_position: Option<u64>,
	) -> Result<FileTransferHandle>
	{
		if let ConnectionState::Connected { con, .. } = &mut self.state {
			con.download_file(channel_id, path, channel_password, seek_position)
		} else {
			bail!("Currently not connected");
		}
	}

	/// Return the size of the part which is already uploaded (when resume is
	/// specified) and a tcp stream where the requested file should be uploaded.
	pub fn upload_file(
		&mut self, channel_id: ChannelId, path: &str,
		channel_password: Option<&str>, size: u64, overwrite: bool,
		resume: bool,
	) -> Result<FileTransferHandle>
	{
		if let ConnectionState::Connected { con, .. } = &mut self.state {
			con.upload_file(
				channel_id,
				path,
				channel_password,
				size,
				overwrite,
				resume,
			)
		} else {
			bail!("Currently not connected");
		}
	}

	/// Get a connection where you can change properties.
	///
	/// Changing properties will send a packet to the server, it will not
	/// immediately change the value.
	pub fn to_mut<'a>(&'a mut self) -> Result<facades::ConnectionMut<'a>> {
		if let ConnectionState::Connected { con, book } = &mut self.state {
			Ok(facades::ConnectionMut { connection: con, inner: book })
		} else {
			bail!("Not connected");
		}
	}

	fn poll_next(
		&mut self, cx: &mut Context,
	) -> Poll<Option<Result<StreamItem>>> {
		if let Some(item) = self.stream_items.pop_front() {
			return Poll::Ready(Some(item));
		}
		match &mut self.state {
			ConnectionState::Connecting(fut) => match fut.poll_unpin(cx) {
				Poll::Pending => Poll::Pending,
				Poll::Ready(Err(ConnectError::Other(e))) => {
					Poll::Ready(Some(Err(e)))
				}
				Poll::Ready(Err(ConnectError::TsError(e))) => {
					Poll::Ready(Some(Err(ConnectError::TsError(e).into())))
				}
				Poll::Ready(Err(ConnectError::IdentityLevelIncrease(
					level,
				))) => {
					if let Err(e) = self.increase_identity_level(level) {
						return Poll::Ready(Some(Err(e)));
					}
					Poll::Ready(Some(Ok(StreamItem::IdentityLevelIncreasing(
						level,
					))))
				}
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
						},
					]))))
				}
			},
			ConnectionState::IdentityLevelIncreasing { recv, .. } => match recv
				.poll_unpin(cx)
			{
				Poll::Pending => Poll::Pending,
				Poll::Ready(Err(e)) => Poll::Ready(Some(Err(format_err!(
					"Failed to receive increased identity level ({:?})",
					e
				)))),
				Poll::Ready(Ok(Err(e))) => Poll::Ready(Some(Err(format_err!(
					"Failed to increase identity level ({:?})",
					e
				)))),
				Poll::Ready(Ok(Ok(identity))) => {
					self.options.identity = Some(identity);
					let fut = Self::connect(
						self.logger.clone(),
						self.options.clone(),
					);
					self.state = ConnectionState::Connecting(Box::pin(fut));
					Poll::Ready(Some(Ok(StreamItem::IdentityLevelIncreased)))
				}
			},
			ConnectionState::Connected { con, book } => match loop {
				match con.client.poll_next_unpin(cx) {
					Poll::Pending => break Poll::Pending,
					Poll::Ready(None) => break Poll::Ready(None),
					Poll::Ready(Some(Err(e))) => {
						warn!(self.logger, "Connection failed, reconnecting";
							"error" => ?e);
						// Reconnect
						// TODO Depending on reason
						let fut = Self::connect(
							self.logger.clone(),
							self.options.clone(),
						);
						self.state = ConnectionState::Connecting(Box::pin(fut));
						return Poll::Ready(Some(Ok(
							StreamItem::DisconnectedTemporarily,
						)));
					}
					Poll::Ready(Some(Ok(item))) => {
						match item {
							ProtoStreamItem::Error(e) => {
								warn!(self.logger, "Connection got a non-fatal error";
									"error" => ?e);
							}
							ProtoStreamItem::Audio(_audio) => {} // TODO
							ProtoStreamItem::Command(cmd) => {
								con.handle_command(
									&self.logger,
									book,
									&mut self.stream_items,
									cmd,
								);
								if let Some(item) =
									self.stream_items.pop_front()
								{
									break Poll::Ready(Some(item));
								}
							}
							_ => {}
						}
					}
				}
			} {
				Poll::Ready(r) => Poll::Ready(r),
				Poll::Pending => {
					// Check file transfers
					let ft =
						con.file_transfers.iter_mut().enumerate().find_map(
							|(i, ft)| match ft.poll_unpin(cx) {
								Poll::Pending => None,
								Poll::Ready(r) => Some((i, r)),
							},
						);
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
	fn drop(&mut self) {
		match &mut self.state {
			ConnectionState::IdentityLevelIncreasing { state, .. } => {
				if let Ok(mut state) = state.lock() {
					*state = IdentityIncreaseLevelState::Canceled;
				}
			}
			ConnectionState::Connected { .. } => {
				// TODO Move connection out of current state if not yet disconnected?
				//tokio::spawn(self.disconnect(Default::default()));
			}
			_ => {}
		}
	}
}

impl<'a> Stream for EventStream<'a> {
	type Item = Result<StreamItem>;
	fn poll_next(
		mut self: Pin<&mut Self>, cx: &mut Context,
	) -> Poll<Option<Self::Item>> {
		self.0.poll_next(cx)
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
				warn!(logger, "Failed to parse message";
					"error" => ?e);
				return;
			}
		};

		// Handle error messages
		if let InMessage::CommandError(e) = &msg {
			for e in e.iter() {
				if let Some(ret_code) =
					e.return_code.as_ref().and_then(|r| r.parse().ok())
				{
					let res =
						if e.id == TsError::Ok { Ok(()) } else { Err(e.id) };
					stream_items.push_back(Ok(StreamItem::MessageResult(
						MessageHandle(ret_code),
						res,
					)));
				}
			}
		} else if let InMessage::FileDownload(msg) = &msg {
			for msg in msg.iter() {
				let ft_id = FileTransferHandle(msg.client_file_transfer_id);
				let ip = msg.ip.unwrap_or_else(|| self.client.address.ip());
				let addr = SocketAddr::new(ip, msg.port);
				let key = msg.file_transfer_key.clone();
				let size = msg.size;

				let fut =
					Box::new(async move {
						let addr = addr;
						let key = key;
						let mut stream = TcpStream::connect(&addr).await?;
						stream.write_all(key.as_bytes()).await?;
						stream.flush().await?;
						Ok(stream)
					})
					.map(move |res| match res {
						Ok(stream) => StreamItem::FileDownload(
							ft_id,
							FileDownloadResult { size, stream },
						),
						Err(e) => StreamItem::FileTransferFailed(ft_id, e),
					});

				self.file_transfers.push(Box::pin(fut));

				// TODO filetransfer
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
					let mut stream = TcpStream::connect(&addr).await?;
					stream.write_all(key.as_bytes()).await?;
					stream.flush().await?;
					Ok(stream)
				})
				.map(move |res| match res {
					Ok(stream) => {
						StreamItem::FileUpload(ft_id, FileUploadResult {
							seek_position,
							stream,
						})
					}
					Err(e) => StreamItem::FileTransferFailed(ft_id, e),
				});

				self.file_transfers.push(Box::pin(fut));
			}
		} else if let InMessage::FileTransferStatus(msg) = &msg {
			for _msg in msg.iter() {
				//let status = FileTransferStatus::Status { status: msg.status };
			}
		} else {
			let events = match book.handle_command(logger, &msg) {
				Ok(r) => r,
				Err(e) => {
					warn!(logger, "Failed to handle message"; "error" => ?e);
					return;
				}
			};
			self.client.hand_back_buffer(cmd.into_buffer());
			stream_items.push_back(Ok(StreamItem::ConEvents(events)));
		}
	}

	// TODO Move return_code handling into tsproto::client
	fn send_command(
		&mut self, mut packet: OutCommand,
	) -> Result<MessageHandle> {
		let code = self.cur_return_code;
		self.cur_return_code += 1;
		packet.write_arg("return_code", &code);
		self.client
			.send_packet(packet.into_packet())
			.map(|_| MessageHandle(code))
	}

	/// Return the size of the file and a tcp stream of the requested file.
	pub fn download_file(
		&mut self, channel_id: ChannelId, path: &str,
		channel_password: Option<&str>, seek_position: Option<u64>,
	) -> Result<FileTransferHandle>
	{
		let ft_id = self.cur_file_transfer_id;
		self.cur_file_transfer_id += 1;
		let packet = c2s::OutFtInitDownloadMessage::new(&mut iter::once(
			c2s::OutFtInitDownloadPart {
				client_file_transfer_id: ft_id,
				name: path,
				channel_id,
				channel_password: channel_password.unwrap_or(""),
				seek_position: seek_position.unwrap_or_default(),
				protocol: 1,
			},
		));

		self.send_command(packet).map(|_| FileTransferHandle(ft_id))
	}

	/// Return the size of the part which is already uploaded (when resume is
	/// specified) and a tcp stream where the requested file should be uploaded.
	pub fn upload_file(
		&mut self, channel_id: ChannelId, path: &str,
		channel_password: Option<&str>, size: u64, overwrite: bool,
		resume: bool,
	) -> Result<FileTransferHandle>
	{
		let ft_id = self.cur_file_transfer_id;
		self.cur_file_transfer_id += 1;

		let packet = c2s::OutFtInitUploadMessage::new(&mut iter::once(
			c2s::OutFtInitUploadPart {
				client_file_transfer_id: ft_id,
				name: path,
				channel_id,
				channel_password: channel_password.unwrap_or(""),
				overwrite,
				resume,
				size,
				protocol: 1,
			},
		));

		self.send_command(packet).map(|_| FileTransferHandle(ft_id))
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
#[derive(Clone, Debug)]
pub struct ConnectOptions {
	address: ServerAddress,
	local_address: Option<SocketAddr>,
	identity: Option<Identity>,
	name: String,
	version: Version,
	channel: Option<String>,
	channel_password: Option<String>,
	password: Option<String>,
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
			name: String::from("TeamSpeakUser"),
			version: Version::Windows_3_X_X__1,
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

	/// Use a password for the given channel when connecting.
	///
	/// # Example
	/// ```
	/// # use tsclientlib::ConnectOptions;
	/// let opts = ConnectOptions::new("localhost")
	///     .channel("Secret Channel".to_string());
	///     .channel_password("My secret password".to_string());
	/// ```
	#[inline]
	pub fn channel_password(mut self, channel_password: String) -> Self {
		self.channel_password = Some(channel_password);
		self
	}

	/// Use a server password when connecting.
	///
	/// # Example
	/// ```
	/// # use tsclientlib::ConnectOptions;
	/// let opts = ConnectOptions::new("localhost").password("My secret password".to_string());
	/// ```
	#[inline]
	pub fn password(mut self, password: String) -> Self {
		self.password = Some(password);
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
}
