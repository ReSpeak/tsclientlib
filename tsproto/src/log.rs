use std::fmt::Debug;
use std::str;

use tracing::{debug, debug_span, Span};
use tsproto_packets::packets::{InUdpPacket, OutUdpPacket, PacketType};
use tsproto_packets::HexSlice;

use crate::connection::{Connection, Event};

fn prepare_span(is_client: bool, incoming: bool) -> Span {
	let dir = if incoming {
		if !cfg!(windows) { "\x1b[1;32mIN\x1b[0m" } else { "IN" }
	} else if !cfg!(windows) {
		"\x1b[1;31mOUT\x1b[0m"
	} else {
		"OUT"
	};
	let to = if is_client { "S" } else { "C" };
	debug_span!("packet", to, %dir)
}

pub fn log_udp_packet(is_client: bool, incoming: bool, packet: &InUdpPacket) {
	let _span = prepare_span(is_client, incoming).entered();
	debug!(header = ?packet.0.header(), content = %HexSlice(packet.0.content()), "UdpPacket");
}

pub fn log_out_udp_packet(is_client: bool, incoming: bool, packet: &OutUdpPacket) {
	let _span = prepare_span(is_client, incoming).entered();
	debug!(
		generation = packet.generation_id(),
		header = ?packet.data().header(),
		content = %HexSlice(packet.data().content()),
		"UdpPacket"
	);
}

pub fn log_packet<P: Debug>(is_client: bool, incoming: bool, packet: &P) {
	// packet.header.c_id is not set for newly created packets so we cannot
	// detect if a packet is incoming or not.
	let _span = prepare_span(is_client, incoming).entered();
	debug!(content = ?packet, "Packet");
}

pub fn log_command(is_client: bool, incoming: bool, p_type: PacketType, cmd: &str) {
	// packet.header.c_id is not set for newly created packets so we cannot
	// detect if a packet is incoming or not.
	let _span = prepare_span(is_client, incoming).entered();
	if p_type == PacketType::Command {
		debug!(content = cmd, "Command");
	} else {
		debug!(content = cmd, "CommandLow");
	}
}

/// Print the content of all packets
///
/// 0 - Print commands
/// 1 - Print packets
/// 2 - Print udp packets
pub fn add_logger_with_verbosity(verbosity: u8, con: &mut Connection) {
	let log_commands = verbosity > 0;
	let log_packets = verbosity > 1;
	let log_udp_packets = verbosity > 2;
	add_logger(log_commands, log_packets, log_udp_packets, con);
}

pub fn add_logger(
	log_commands: bool, log_packets: bool, log_udp_packets: bool, con: &mut Connection,
) {
	if log_commands || log_packets || log_udp_packets {
		let is_client = con.is_client;
		let listener = Box::new(move |event: &Event| match event {
			Event::ReceiveUdpPacket(packet) => {
				if log_udp_packets {
					log_udp_packet(is_client, true, packet);
				}
			}
			Event::ReceivePacket(packet) => {
				if log_packets {
					log_packet(is_client, true, packet);
				} else if log_commands {
					let p_type = packet.header().packet_type();
					if p_type.is_command() {
						if let Ok(s) = str::from_utf8(packet.content()) {
							log_command(is_client, true, p_type, s);
						}
					}
				}
			}
			Event::SendUdpPacket(packet) => {
				if log_udp_packets {
					log_out_udp_packet(is_client, false, packet);
				}
			}
			Event::SendPacket(packet) => {
				if log_packets {
					log_packet(is_client, false, &packet.packet());
				} else if log_commands {
					let p_type = packet.header().packet_type();
					if p_type.is_command() {
						if let Ok(s) = str::from_utf8(packet.content()) {
							log_command(is_client, false, p_type, s);
						}
					}
				}
			}
		});

		con.event_listeners.push(listener);
	}
}
