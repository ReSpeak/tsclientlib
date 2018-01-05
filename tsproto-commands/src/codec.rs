//! This module contains a stream and a sink which convert Packets to Commands.
use futures::{future, Sink, stream, Stream};
use slog::Logger;
use tsproto::commands::Command;
use tsproto::errors::Error;
use tsproto::packets::{Data, Header, Packet, PacketType};

use messages::Notification;

/// Convert a stream/sink of `Packet`s to a stream of `Notification`s.
pub struct CommandCodec;

impl CommandCodec {
	pub fn new_stream<Inner: Stream<Item = Packet, Error = Error> + 'static>(
		inner: Inner, logger: Logger) -> Box<Stream<Item = Notification,
			Error = Error>> {
		Box::new(inner.and_then(move |p| {
			let res: Box<Stream<Item=_, Error=_>> = match p.data {
				Data::Command(cmd) |
				Data::CommandLow(cmd) => {
					let mut cmds = cmd.get_commands();
					let cmds: Vec<_> = cmds.drain(..).flat_map(|c|
						match Notification::parse(c) {
							Ok(n) => Some(n),
							Err(e) => {
								warn!(logger, "Error parsing packet";
									  "error" => ?e);
								None
							}
						}).collect();
					Box::new(stream::iter_ok(cmds))
				}
				_ => Box::new(stream::empty())
			};
			future::ok(res)
		}).flatten())
	}

	pub fn new_sink<
		Inner: Sink<SinkItem = Packet, SinkError = Error> + 'static>(
		inner: Inner) -> Box<Sink<SinkItem = Notification, SinkError = Error>> {
		Box::new(inner.with(|n: Notification| {
			// TODO Use Command or CommandLow, set newprotocol flag
			let cmd: Command = panic!(); //n.into();
			let header = Header::new(PacketType::Command);
			future::ok(Packet::new(header, Data::Command(cmd)))
		}))
	}
}
