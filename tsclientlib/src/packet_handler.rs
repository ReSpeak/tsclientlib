use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use chashmap::CHashMap;
use futures::sync::oneshot;
use futures::{Async, Future, Poll, Stream, task};
use parking_lot::RwLock;
use slog::Logger;
use tsproto::handler_data::ConnectionValue;
use tsproto::packets::*;
use tsproto_commands::messages::Message;
use {tokio, tsproto, PHBox, TsError};

use data::Connection;

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
	connection_recv: Option<oneshot::Receiver<Arc<RwLock<Connection>>>>,
	pub(crate) return_codes: Arc<ReturnCodeHandler>,
}

struct SimplePacketStreamHandler<Inner: Stream<Item=InCommand, Error=tsproto::Error>> {
	inner: Inner,
	logger: Logger,
	initserver_sender: Option<oneshot::Sender<InCommand>>,
	connection_recv: Option<oneshot::Receiver<Arc<RwLock<Connection>>>>,
	connection: Option<Arc<RwLock<Connection>>>,
	return_codes: Arc<ReturnCodeHandler>,
}

impl SimplePacketHandler {
	pub(crate) fn new(
		logger: Logger,
		handle_packets: Option<PHBox>,
		initserver_sender: oneshot::Sender<InCommand>,
		connection_recv: oneshot::Receiver<Arc<RwLock<Connection>>>,
	) -> Self {
		Self {
			logger,
			handle_packets,
			initserver_sender: Some(initserver_sender),
			connection_recv: Some(connection_recv),
			return_codes: Arc::new(ReturnCodeHandler {
				return_codes: CHashMap::new(),
				cur_return_code: AtomicUsize::new(0),
			}),
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
		tokio::spawn(s2c_init_stream.for_each(|_| Ok(())).map_err(|e| {
			println!("Init stream exited with error ({:?})", e)
		}));

		let handler = SimplePacketStreamHandler {
			inner: command_stream,
			logger: self.logger.clone(),
			initserver_sender: self.initserver_sender.take(),
			connection_recv: self.connection_recv.take(),
			connection: None,
			return_codes: self.return_codes.clone(),
		};

		if let Some(h) = &mut self.handle_packets {
			h.new_connection(Box::new(handler), Box::new(audio_stream));
		} else {
			let logger = self.logger.clone();
			tokio::spawn(handler.for_each(|_| Ok(())).map_err(
				move |e| {
					error!(logger, "Command stream exited with error ({:?})", e)
				},
			));
			let logger = self.logger.clone();
			tokio::spawn(audio_stream.for_each(|_| Ok(())).map_err(move |e| {
				error!(logger, "Audio stream exited with error ({:?})", e)
			}));
		}
	}
}

impl<Inner: Stream<Item=InCommand, Error=tsproto::Error>> Stream for SimplePacketStreamHandler<Inner> {
	type Item = InCommand;
	type Error = tsproto::Error;

	/// 1. Get first packet and send with `initserver_sender`
	/// 2. Get connection by polling `connection_recv`
	/// 3. For each command packet
	/// 3.1 If it is an error response: Send return code if possible
	/// 3.2 Else split up into messages and apply to connection
	///	4. Output packet
	fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
		let con;
		if let Some(con) = &self.connection {
			let cmd = if let Some(p) = try_ready!(self.inner.poll()) {
				p
			} else {
				return Ok(Async::Ready(None));
			};

			// 3.
			let mut con = con.write();
			let mut handled = true;
			// Split into messages
			for c in cmd.iter() {
				match Message::parse(c) {
					Err(e) => {
						warn!(self.logger, "Failed to parse message";
							"command" => cmd.name(),
							"error" => ?e);
						handled = false;
					}
					Ok(Message::CommandError(cmd)) => {
						// 3.1
						if let Ok(code) = cmd.return_code.parse() {
							if let Some(return_sender) = self
								.return_codes.return_codes
								.remove(&code) {
								// Ignore if sending fails
								let _ = return_sender.send(cmd.id).is_err();
							}
						}
					}
					Ok(msg) => {
						// 3.2
						// Apply
						if let Err(e) = con.handle_message(&msg) {
							warn!(self.logger, "Failed to handle message";
								"command" => cmd.name(),
								"error" => ?e);
						}
						handled = false;
					}
				}
			}
			if handled {
				// Packet contains only handled return codes
				task::current().notify();
				return Ok(Async::NotReady);
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
						error!(self.logger, "Sending the initserver packet \
							from the packet handler failed");
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
				unreachable!("SimplePacketStreamHandler received connection but it \
					is not set");
			}
		}
		// Also 2.
		self.connection_recv = None;
		self.connection = Some(con);
		task::current().notify();
		return Ok(Async::NotReady);
	}
}
