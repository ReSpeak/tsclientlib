use std::net::SocketAddr;

use slog::Logger;

use packets::{Packet, UdpPacket};

pub struct PacketLogger;
impl PacketLogger {
	fn prepare_logger(
		logger: &Logger,
		is_client: bool,
		incoming: bool,
	) -> Logger {
		let in_s = if incoming {
			if !cfg!(windows) {
				"\x1b[1;32mIN\x1b[0m"
			} else {
				"IN"
			}
		} else if !cfg!(windows) {
			"\x1b[1;31mOUT\x1b[0m"
		} else {
			"OUT"
		};
		let to_s = if is_client { "S" } else { "C" };
		logger.new(o!("to" => to_s, "dir" => in_s))
	}

	pub fn log_udp_packet(
		logger: &Logger,
		addr: SocketAddr,
		is_client: bool,
		packet: &UdpPacket,
	) {
		let logger = Self::prepare_logger(
			&logger.new(o!("addr" => addr)),
			is_client,
			is_client != packet.from_client,
		);
		debug!(logger, "UdpPacket"; "content" => ?packet);
	}

	pub fn log_packet(
		logger: &Logger,
		is_client: bool,
		incoming: bool,
		packet: &Packet,
	) {
		// packet.header.c_id is not set for newly created packets so we cannot
		// detect if a packet is incoming or not.
		let logger = Self::prepare_logger(logger, is_client, incoming);
		debug!(logger, "Packet"; "content" => ?packet);
	}
}
