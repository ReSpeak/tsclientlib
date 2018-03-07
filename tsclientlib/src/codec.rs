use std::cell::RefCell;
use std::rc::Rc;

use futures::{future, Sink, stream, Stream};
use tsproto::Error;
use tsproto::commands::Command;
use tsproto::connectionmanager::ConnectionManager;
use tsproto::handler_data;
use tsproto::packets::{Data, Header, Packet, PacketType};

use tsproto_commands::messages::Notification;

/// A "high-level" packet, which contains either a notification or audio.
#[derive(Debug, Clone)]
pub enum Message {
	Notification(Notification),
	/// Not yet implemented
	/// Lazily decoded/transcoded? (for sending and receiving)
	/// So we do not decode + encode if the source format is the destination
	/// format.
	Audio(),
}

/// Convert a stream/sink of `Packet`s to a stream of `Notification`s.
pub struct CommandCodec;

impl CommandCodec {
	pub fn new_stream<CM: ConnectionManager + 'static>(
		data: &Rc<RefCell<handler_data::Data<CM>>>)
		-> Box<Stream<Item = (CM::ConnectionsKey, Message), Error = Error>> {
		let logger = data.borrow().logger.clone();
		let packets = handler_data::Data::get_packets(Rc::downgrade(data));

		Box::new(packets.and_then(move |(con_key, p)| {
			let res: Box<Stream<Item=_, Error=_>> = match p.data {
				Data::Command(cmd) | Data::CommandLow(cmd) => {
					let mut cmds = cmd.get_commands();
					let cmds: Vec<_> = cmds.drain(..).flat_map(|c|
						match Notification::parse(c) {
							Ok(n) => Some((con_key.clone(),
								Message::Notification(n))),
							Err(e) => {
								warn!(logger, "Error parsing command";
									"command" => %cmd.command,
									"error" => ?e,
								);
								None
							}
						}).collect();
					Box::new(stream::iter_ok(cmds))
				}
				Data::Voice { .. } | Data::VoiceWhisper { .. } => {
					Box::new(stream::once(Ok((con_key, Message::Audio()))))
				}
				// Ignore other packets
				_ => Box::new(stream::empty()),
			};
			future::ok(res)
		}).flatten())
	}

	pub fn new_sink<CM: ConnectionManager + 'static>(
		data: &Rc<RefCell<handler_data::Data<CM>>>) -> Box<Sink<
		SinkItem = (CM::ConnectionsKey, Message), SinkError = Error>> {
		let packets = handler_data::Data::get_packets(Rc::downgrade(data));

		Box::new(packets.with(|(_con_key, _msg)| {
			// TODO Use Command or CommandLow, set newprotocol flag
			let _cmd: Command = panic!(); //n.into();
			let _header = Header::new(PacketType::Command);
			future::ok((_con_key, Packet::new(_header, Data::Command(_cmd))))
		}))
	}
}
