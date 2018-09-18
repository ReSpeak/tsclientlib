use std::fmt::Display;
use std::net::SocketAddr;

use slog::Logger;

use packets::{Packet, UdpPacket};

pub struct PacketLogger;
impl PacketLogger {
    fn prepare_logger<Id: Display>(
        logger: &Logger,
        id: &Id,
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
        let id_s = format!("{}", id);
        logger.new(o!("addr" => id_s, "dir" => in_s, "to" => to_s))
    }

    pub fn log_udp_packet(
        logger: &Logger,
        addr: SocketAddr,
        is_client: bool,
        incoming: bool,
        packet: &UdpPacket,
    ) {
        let logger = Self::prepare_logger(logger, &addr, is_client, incoming);
        debug!(logger, "UdpPacket"; "content" => ?packet);
    }

    pub fn log_packet<Id: Display>(
        logger: &Logger,
        id: &Id,
        is_client: bool,
        incoming: bool,
        packet: &Packet,
    ) {
        let logger = Self::prepare_logger(logger, id, is_client, incoming);
        debug!(logger, "Packet"; "content" => ?packet);
    }
}
