use std::fmt::Debug;
use std::str;

use slog::{debug, o, Logger};
use tsproto_packets::packets::{InUdpPacket, OutUdpPacket, PacketType};
use tsproto_packets::HexSlice;

use crate::connection::{Connection, Event};

fn prepare_logger(logger: &Logger, is_client: bool, incoming: bool) -> Logger {
	let in_s = if incoming {
		if !cfg!(windows) { "\x1b[1;32mIN\x1b[0m" } else { "IN" }
	} else if !cfg!(windows) {
		"\x1b[1;31mOUT\x1b[0m"
	} else {
		"OUT"
	};
	let to_s = if is_client { "S" } else { "C" };
	logger.new(o!("to" => to_s, "dir" => in_s))
}

pub fn log_udp_packet(logger: &Logger, is_client: bool, incoming: bool, packet: &InUdpPacket) {
	let logger = prepare_logger(logger, is_client, incoming);
	debug!(logger, "UdpPacket"; "header" => ?packet.0.header(),
		"content" => %HexSlice(packet.0.content()),
	);
}

pub fn log_out_udp_packet(logger: &Logger, is_client: bool, incoming: bool, packet: &OutUdpPacket) {
	let logger = prepare_logger(logger, is_client, incoming);
	debug!(logger, "UdpPacket";
		"generation" => packet.generation_id(),
		"header" => ?packet.data().header(),
		"content" => %HexSlice(packet.data().content()),
	);
}

pub fn log_packet<P: Debug>(logger: &Logger, is_client: bool, incoming: bool, packet: &P) {
	// packet.header.c_id is not set for newly created packets so we cannot
	// detect if a packet is incoming or not.
	let logger = prepare_logger(logger, is_client, incoming);
	debug!(logger, "Packet"; "content" => ?packet);
}

pub fn log_command(
	logger: &Logger, is_client: bool, incoming: bool, p_type: PacketType, cmd: &str,
) {
	// packet.header.c_id is not set for newly created packets so we cannot
	// detect if a packet is incoming or not.
	let logger = prepare_logger(logger, is_client, incoming);
	if p_type == PacketType::Command {
		debug!(logger, "Command"; "content" => cmd);
	} else {
		debug!(logger, "CommandLow"; "content" => cmd);
	}
}

/// Print the content of all packets
///
/// 0 - Print commands
/// 1 - Print packets
/// 2 - Print udp packets
pub fn add_logger(logger: Logger, verbosity: u8, con: &mut Connection) {
	let is_client = con.is_client;
	let listener = Box::new(move |event: &Event| match event {
		Event::ReceiveUdpPacket(packet) => {
			if verbosity > 0 {
				log_udp_packet(&logger, is_client, true, packet);
			}
		}
		Event::ReceivePacket(packet) => {
			if verbosity > 1 {
				log_packet(&logger, is_client, true, packet);
			} else {
				let p_type = packet.header().packet_type();
				if p_type.is_command() {
					if let Ok(s) = str::from_utf8(packet.content()) {
						log_command(&logger, is_client, true, p_type, s);
					}
				}
			}
		}
		Event::SendUdpPacket(packet) => {
			if verbosity > 0 {
				log_out_udp_packet(&logger, is_client, false, packet);
			}
		}
		Event::SendPacket(packet) => {
			if verbosity > 1 {
				log_packet(&logger, is_client, false, &packet.packet());
			} else {
				let p_type = packet.header().packet_type();
				if p_type.is_command() {
					if let Ok(s) = str::from_utf8(packet.content()) {
						log_command(&logger, is_client, false, p_type, s);
					}
				}
			}
		}
	});

	con.event_listeners.push(listener);
}
