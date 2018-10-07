use futures::{future, Sink, stream, Stream};
use tsproto::Error;
use tsproto::commands::Command;
use tsproto::packets::{Data, Header, Packet, PacketType};

use tsproto_commands::messages;

/// A "high-level" packet, which contains either a notification or audio.
///
/// The content is boxed because `Message` objects tend to be larger than
/// `Audio` objects.
#[derive(Debug, Clone)]
pub enum Message {
	Message(Box<messages::Message>),
	/// Not yet implemented
	/// Lazily decoded/transcoded? (for sending and receiving)
	/// So we do not decode + encode if the source format is the destination
	/// format.
	Audio(),
}

/// Convert a stream/sink of `Packet`s to a stream of `Message`s.
pub struct CommandCodec;

#[cfg(TODO)]
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
						match messages::Message::parse(c) {
							Ok(n) => Some((con_key.clone(),
								Message::Message(Box::new(n)))),
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
				Data::VoiceS2C { .. } | Data::VoiceWhisperS2C { .. } => {
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
