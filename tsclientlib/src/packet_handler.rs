use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use chashmap::CHashMap;
use failure::format_err;
use futures::sync::oneshot;
use futures::{task, try_ready, Async, Future, Poll, Stream};
use slog::{error, warn, Logger};
use tsproto::handler_data::ConnectionValue;
use tsproto_packets::packets::*;
use ts_bookkeeping::messages::s2c::{InCommandError, InMessageTrait};
use tsproto_packets::packets::InCommand;

use crate::{Connection, PHBox, TsError};
use crate::filetransfer::FileTransferHandler;

pub(crate) struct ReturnCodeHandler {
	return_codes: CHashMap<usize, oneshot::Sender<TsError>>,
	cur_return_code: AtomicUsize,
}

/// **This is part of the unstable interface.**
///
/// You can use it if you need access to lower level functions, but this
/// interface may change on any version changes.
#[doc(hidden)]
pub struct SimplePacketHandler {
	logger: Logger,
	handle_packets: Option<PHBox>,
	initserver_sender: Option<oneshot::Sender<InCommand>>,
	connection_recv: Option<oneshot::Receiver<Connection>>,
	pub(crate) return_codes: Arc<ReturnCodeHandler>,
	pub(crate) ft_ids: Arc<FileTransferHandler>,
}

struct SimplePacketStreamHandler<
	Inner: Stream<Item = InCommand, Error = tsproto::Error>,
> {
	inner: Inner,
	logger: Logger,
	initserver_sender: Option<oneshot::Sender<InCommand>>,
	connection_recv: Option<oneshot::Receiver<Connection>>,
	connection: Option<Connection>,
	return_codes: Arc<ReturnCodeHandler>,
	ft_ids: Arc<FileTransferHandler>,
}

impl SimplePacketHandler {
	pub(crate) fn new(
		logger: Logger,
		handle_packets: Option<PHBox>,
		initserver_sender: oneshot::Sender<InCommand>,
		connection_recv: oneshot::Receiver<Connection>,
	) -> Self
	{
		Self {
			logger,
			handle_packets,
			initserver_sender: Some(initserver_sender),
			connection_recv: Some(connection_recv),
			return_codes: Arc::new(ReturnCodeHandler {
				return_codes: CHashMap::new(),
				cur_return_code: AtomicUsize::new(0),
			}),
			ft_ids: Arc::new(FileTransferHandler::new()),
		}
	}
}

impl ReturnCodeHandler {
	/// Get a return code and a receiver which gets notified when an answer is
	/// received.
	pub(crate) fn get_return_code(
		&self,
	) -> (usize, oneshot::Receiver<TsError>) {
		let code = self.cur_return_code.fetch_add(1, Ordering::Relaxed);
		let (send, recv) = oneshot::channel();
		// The receiver should fail when the sender is dropped, but usize should
		// be enough for every platform.
		self.return_codes.insert(code, send);
		(code, recv)
	}
}

impl<T: 'static> tsproto::handler_data::PacketHandler<T>
	for SimplePacketHandler
{
	fn new_connection<S1, S2, S3, S4>(
		&mut self,
		_: &ConnectionValue<T>,
		s2c_init_stream: S1,
		_c2s_init_stream: S2,
		command_stream: S3,
		audio_stream: S4,
	) where
		S1: Stream<Item = InS2CInit, Error = tsproto::Error> + Send + 'static,
		S2: Stream<Item = InC2SInit, Error = tsproto::Error> + Send + 'static,
		S3: Stream<Item = InCommand, Error = tsproto::Error> + Send + 'static,
		S4: Stream<Item = InAudio, Error = tsproto::Error> + Send + 'static,
	{
		// Ignore c2s init stream and start s2c init stream
		tokio::spawn(
			s2c_init_stream.for_each(|_| Ok(())).map_err(|e| {
				println!("Init stream exited with error ({:?})", e)
			}),
		);

		let handler = SimplePacketStreamHandler {
			inner: command_stream,
			logger: self.logger.clone(),
			initserver_sender: self.initserver_sender.take(),
			connection_recv: self.connection_recv.take(),
			connection: None,
			return_codes: self.return_codes.clone(),
			ft_ids: self.ft_ids.clone(),
		};

		if let Some(h) = &mut self.handle_packets {
			h.new_connection(Box::new(handler), Box::new(audio_stream));
		} else {
			let logger = self.logger.clone();
			tokio::spawn(handler.for_each(|_| Ok(())).map_err(move |e| {
				error!(logger, "Command stream exited with error ({:?})", e)
			}));
			let logger = self.logger.clone();
			tokio::spawn(audio_stream.for_each(|_| Ok(())).map_err(move |e| {
				error!(logger, "Audio stream exited with error ({:?})", e)
			}));
		}
	}
}

impl<Inner: Stream<Item = InCommand, Error = tsproto::Error>> Stream
	for SimplePacketStreamHandler<Inner>
{
	type Item = InCommand;
	type Error = tsproto::Error;

	/// 1. Get first packet and send with `initserver_sender`
	/// 2. Get connection by polling `connection_recv`
	/// 3.1 If it is an error response: Send return code if possible
	///     Handle file transfer commands
	/// 3.2 Else apply message to connection
	///	4. Output packet
	fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
		let con;
		if let Some(connection) = &self.connection {
			let cmd = if let Some(p) = try_ready!(self.inner.poll()) {
				p
			} else {
				return Ok(Async::Ready(None));
			};

			// 3.
			let mut con = connection.inner.connection.write();
			let name = cmd.name().to_string();
			// 3.1
			if cmd.name() == "error" {
				let msg = InCommandError::new(&cmd)
					.map_err(|e| format_err!("Cannot parse error ({:?})", e))?;
				let msg = msg.iter().next().unwrap();

				if let Some(code) = msg.return_code {
					if let Some(return_sender) =
						self.return_codes.return_codes.remove(&code.parse()?)
					{
						// Ignore if sending fails
						let _ = return_sender.send(msg.id).is_err();

						// Packet contains only handled return codes
						task::current().notify();
						return Ok(Async::NotReady);
					}
				}
			} else if cmd.name() == "notifystartdownload" {
				if self.ft_ids.handle_start_download(&cmd)? {
					// Packet contains only handled data
					task::current().notify();
					return Ok(Async::NotReady);
				}
			} else if cmd.name() == "notifystartupload" {
				if self.ft_ids.handle_start_upload(&cmd)? {
					// Packet contains only handled data
					task::current().notify();
					return Ok(Async::NotReady);
				}
			} else if cmd.name() == "notifystatusfiletransfer" {
				if self.ft_ids.handle_status(&cmd)? {
					// Packet contains only handled data
					task::current().notify();
					return Ok(Async::NotReady);
				}
			}

			// 3.2
			// Apply
			let events = match con.handle_command(&cmd) {
				Ok(e) => {
					if e.len() > 0 {
						Some(e)
					} else {
						None
					}
				}
				Err(e) => {
					warn!(self.logger, "Failed to handle message";
						"command" => name,
						"error" => ?e);
					None
				}
			};

			// Call event handler
			drop(con);
			if let Some(events) = events {
				let con = connection.lock();
				let listeners = connection.inner.event_listeners.read();
				let ev = crate::Event::ConEvents(&con, &events);
				for l in listeners.values() {
					l(&ev);
				}
			}

			// 4.
			return Ok(Async::Ready(Some(cmd)));
		} else {
			if self.initserver_sender.is_some() {
				// 1.
				let cmd = if let Some(p) = try_ready!(self.inner.poll()) {
					p
				} else {
					return Ok(Async::Ready(None));
				};

				if let Some(send) = self.initserver_sender.take() {
					if send.send(cmd).is_err() {
						error!(
							self.logger,
							"Sending the initserver packet from the packet \
							 handler failed"
						);
					}
					task::current().notify();
					return Ok(Async::NotReady);
				} else {
					unreachable!();
				}
			} else if let Some(con_recv) = &mut self.connection_recv {
				// 2.
				con = try_ready!(con_recv.poll());
			} else {
				unreachable!(
					"SimplePacketStreamHandler received connection but it is \
					 not set"
				);
			}
		}
		// Also 2.
		self.connection_recv = None;
		self.connection = Some(con);
		task::current().notify();
		return Ok(Async::NotReady);
	}
}
