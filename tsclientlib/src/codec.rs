use slog::Logger;
use tsproto::packets::{Data, Packet};
use tsproto_commands::messages::Message;

fn packet2msg(logger: Logger, packet: Packet) -> Vec<Message> {
	match packet.data {
		Data::Command(cmd) | Data::CommandLow(cmd) => {
			let mut cmds = cmd.get_commands();
			let res = cmds.drain(..).flat_map(|c|
				match Message::parse(c) {
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
