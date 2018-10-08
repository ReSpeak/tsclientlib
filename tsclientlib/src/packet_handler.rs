use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use chashmap::CHashMap;
use futures::sync::{mpsc, oneshot};
use futures::{Future, Stream};
use num::FromPrimitive;
use slog::Logger;
use tsproto::commands::Command;
use tsproto::handler_data::ConnectionValue;
use tsproto::packets::{Data, Packet};
use tsproto_commands::messages::Message;
use {tokio, tsproto, PHBox, TsError};

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
	pub(crate) handle_packets: Option<PHBox>,
	pub(crate) initserver_sender: Option<mpsc::Sender<Command>>,
	pub(crate) return_codes: Arc<ReturnCodeHandler>,
}

impl SimplePacketHandler {
	pub(crate) fn new(logger: Logger) -> Self {
		Self {
			logger,
			handle_packets: None,
			initserver_sender: None,
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
		c: &ConnectionValue<T>,
		command_stream: S1,
		audio_stream: S2,
	) where
		S1: Stream<Item = Packet, Error = tsproto::Error> + Send + 'static,
		S2: Stream<Item = Packet, Error = tsproto::Error> + Send + 'static,
	{
		let logger = c.mutex.lock().unwrap().1.logger.clone();
		let mut send = self.initserver_sender.clone();
		let return_codes = self.return_codes.clone();
		let command_stream = command_stream
			.map(move |p| {
				let is_cmd;
				if let Packet {
					data: Data::Command(cmd),
					..
				} = &p
				{
					is_cmd = true;
					if cmd.command == "error" {
						let cmd = cmd.get_commands().remove(0);
						if cmd.has_arg("id") && cmd.has_arg("return_code") {
							if let Ok(code) = cmd.args["return_code"].parse() {
								if let Some(return_sender) =
									return_codes.return_codes.remove(&code)
								{
									if let Ok(e_code) = cmd.args["id"].parse() {
										if let Some(e) =
											TsError::from_u32(e_code)
										{
											// Ignore if sending fails
											if return_sender.send(e).is_ok() {
												return None;
											}
										} else {
											error!(logger, "Unknown error id";
												"error_id" => e_code);
										}
									} else {
										error!(logger, "Unknown error id";
											"error_id" => cmd.args["id"]);
									}
								}
							}
						}
					}
				} else {
					is_cmd = false;
				}

				if is_cmd {
					if let Some(mut send) = send.take() {
						if let Packet {
							data: Data::Command(cmd),
							..
						} = p
						{
							// Don't block, we should only send 1 command
							let _ = send.try_send(cmd);
							return None;
						} else {
							unreachable!();
						}
					}
				}
				Some(p)
			}).filter_map(|p| p);

		if let Some(h) = &mut self.handle_packets {
			h.new_connection(Box::new(command_stream), Box::new(audio_stream));
		} else {
			let logger = self.logger.clone();
			tokio::spawn(command_stream.for_each(|_| Ok(())).map_err(
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
