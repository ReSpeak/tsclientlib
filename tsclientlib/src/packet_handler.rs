use std::sync::{Arc, RwLock};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use chashmap::CHashMap;
use futures::sync::oneshot;
use futures::{Async, Future, Poll, Stream, task};
use slog::Logger;
use tsproto::commands::Command;
use tsproto::handler_data::ConnectionValue;
use tsproto::packets::{Data, Packet};
use tsproto_commands::messages::Message;
use {tokio, tsproto, PHBox, TsError};

use data::Connection;

fn packet2msg(logger: Logger, packet: Packet) -> Vec<Message> {
	match packet.data {
		Data::Command(cmd) | Data::CommandLow(cmd) => {
			let mut cmds = cmd.get_commands();
			let res = cmds
				.drain(..)
				.flat_map(|c| match Message::parse(c) {
					Ok(n) => Some(n),
					Err(e) => {
						warn!(logger, "Error parsing command";
							"command" => %cmd.command,
							"error" => ?e,
						);
						None
					}
				}).collect();
			res
		}
		_ => Vec::new(),
	}
}

pub(crate) struct ReturnCodeHandler {
	return_codes: CHashMap<usize, oneshot::Sender<TsError>>,
	cur_return_code: AtomicUsize,
}

pub(crate) struct SimplePacketHandler {
	logger: Logger,
	log_commands: Arc<AtomicBool>,
	handle_packets: Option<PHBox>,
	initserver_sender: Option<oneshot::Sender<Command>>,
	connection_recv: Option<oneshot::Receiver<Arc<RwLock<Connection>>>>,
	pub(crate) return_codes: Arc<ReturnCodeHandler>,
}

struct SimplePacketStreamHandler<Inner: Stream<Item=Packet, Error=tsproto::Error>> {
	inner: Inner,
	logger: Logger,
	log_commands: Arc<AtomicBool>,
	initserver_sender: Option<oneshot::Sender<Command>>,
	connection_recv: Option<oneshot::Receiver<Arc<RwLock<Connection>>>>,
	connection: Option<Arc<RwLock<Connection>>>,
	return_codes: Arc<ReturnCodeHandler>,
}

impl SimplePacketHandler {
	pub(crate) fn new(
		logger: Logger,
		log_commands: Arc<AtomicBool>,
		handle_packets: Option<PHBox>,
		initserver_sender: oneshot::Sender<Command>,
		connection_recv: oneshot::Receiver<Arc<RwLock<Connection>>>,
	) -> Self {
		Self {
			logger,
			log_commands,
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
	fn new_connection<S1, S2>(
		&mut self,
		_: &ConnectionValue<T>,
		command_stream: S1,
		audio_stream: S2,
	) where
		S1: Stream<Item = Packet, Error = tsproto::Error> + Send + 'static,
		S2: Stream<Item = Packet, Error = tsproto::Error> + Send + 'static,
	{
		let handler = SimplePacketStreamHandler {
			inner: command_stream,
			logger: self.logger.clone(),
			log_commands: self.log_commands.clone(),
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

impl<Inner: Stream<Item=Packet, Error=tsproto::Error>> Stream for SimplePacketStreamHandler<Inner> {
	type Item = Packet;
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
			let packet = if let Some(p) = try_ready!(self.inner.poll()) {
				p
			} else {
				return Ok(Async::Ready(None));
			};

			match &packet.data {
				Data::Command(cmd) | Data::CommandLow(cmd) => {
					if self.log_commands.load(Ordering::Relaxed) {
						let mut v = Vec::new();
						cmd.write(&mut v).unwrap();
						debug!(self.logger, "Command"; "cmd" =>
							::std::str::from_utf8(&v).unwrap());
					}
					// 3.
					let mut con = con.write().unwrap();
					let mut handled = true;
					// Split into messages
					for cmd in cmd.get_commands() {
						match Message::parse(cmd) {
							Err(e) => {
								warn!(self.logger, "Failed to parse message";
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
				}
				_ => unreachable!("Got wrong packet type in command handler"),
			}

			// 4.
			return Ok(Async::Ready(Some(packet)));
		} else {
			if self.initserver_sender.is_some() {
				// 1.
				let packet = if let Some(p) = try_ready!(self.inner.poll()) {
					p
				} else {
					return Ok(Async::Ready(None));
				};

				if let Some(send) = self.initserver_sender.take() {
					match packet.data {
						Data::Command(cmd) | Data::CommandLow(cmd) => {
							if send.send(cmd).is_err() {
								error!(self.logger, "Sending the initserver packet \
									from the packet handler failed");
							}
						}
						// TODO Not unreachable??
						_ => unreachable!("Got wrong packet type in command handler"),
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
