use std::collections::HashMap;

use anyhow::format_err;
use futures::prelude::*;
use slog::{error, warn, Logger};
use tokio::sync::oneshot;
use ts_bookkeeping::messages::s2c::InCommandError;
use tsproto_packets::packets::InCommand;

use crate::filetransfer::FileTransferHandler;
use crate::{Connection, TsError};

pub(crate) struct ReturnCodeHandler {
	return_codes: HashMap<usize, oneshot::Sender<TsError>>,
	cur_return_code: u16,
}

pub(crate) struct SimplePacketHandler {
	logger: Logger,
	pub(crate) return_codes: ReturnCodeHandler,
	pub(crate) ft_ids: FileTransferHandler,
}

/*struct SimplePacketStreamHandler<
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
			let mut con = connection.inner.connection.write().unwrap();
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
				let listeners = connection.inner.event_listeners.read().unwrap();
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
}*/
