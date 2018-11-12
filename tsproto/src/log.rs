use std::marker::PhantomData;
use std::net::SocketAddr;

use bytes;
use slog::Logger;

use commands::Command;
use connection::Connection;
use connectionmanager::ConnectionManager;
use handler_data::{Data, PacketObserver, UdpPacketObserver};
use packets::{self, Packet, PacketType, UdpPacket};

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
	let logger = prepare_logger(
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
	let logger = prepare_logger(logger, is_client, incoming);
	debug!(logger, "Packet"; "content" => ?packet);
}

pub fn log_command(
	logger: &Logger,
	is_client: bool,
	incoming: bool,
	p_type: PacketType,
	cmd: &Command,
) {
	// packet.header.c_id is not set for newly created packets so we cannot
	// detect if a packet is incoming or not.
	let logger = prepare_logger(logger, is_client, incoming);
	let mut v = Vec::new();
	cmd.write(&mut v).unwrap();
	let cmd_s = ::std::str::from_utf8(&v).unwrap();
	if p_type == PacketType::Command {
		debug!(logger, "Command"; "content" => cmd_s);
	} else {
		debug!(logger, "CommandLow"; "content" => cmd_s);
	}
}

#[derive(Clone, Debug)]
struct UdpPacketLogger {
	logger: Logger,
	is_client: bool,
	incoming: bool,
}
impl UdpPacketObserver for UdpPacketLogger {
	fn observe(&self, addr: SocketAddr, udp_packet: &bytes::Bytes) {
		let udp_packet = UdpPacket::new(&*udp_packet,
			// from_client
			self.is_client != self.incoming);
		log_udp_packet(&self.logger, addr, self.is_client, &udp_packet);
	}
}

#[derive(Clone, Debug)]
struct PacketLogger<T: Send> {
	is_client: bool,
	incoming: bool,
	phantom_data: PhantomData<T>,
}
impl<T: Send> PacketObserver<T> for PacketLogger<T> {
	fn observe(&self, con: &mut (T, Connection), packet: &mut Packet) {
		log_packet(&con.1.logger, self.is_client, self.incoming, packet);
	}
}

#[derive(Clone, Debug)]
struct CommandLogger<T: Send> {
	is_client: bool,
	incoming: bool,
	phantom_data: PhantomData<T>,
}
impl<T: Send> PacketObserver<T> for CommandLogger<T> {
	fn observe(&self, con: &mut (T, Connection), packet: &mut Packet) {
		match &packet.data {
			packets::Data::Command(cmd) | packets::Data::CommandLow(cmd) => {
				log_command(&con.1.logger, self.is_client, self.incoming,
					packet.header.get_type(), cmd);
			}
			_ => {}
		}
	}
}

pub fn add_udp_packet_logger<CM: ConnectionManager + 'static>(data: &mut Data<CM>) {
	data.add_udp_packet_observer(true, "log".into(), Box::new(UdpPacketLogger {
		logger: data.logger.clone(),
		is_client: data.is_client,
		incoming: true,
	}));
	data.add_udp_packet_observer(false, "log".into(), Box::new(UdpPacketLogger {
		logger: data.logger.clone(),
		is_client: data.is_client,
		incoming: false,
	}));
}

pub fn add_packet_logger<CM: ConnectionManager + 'static>(data: &mut Data<CM>) {
	data.add_packet_observer(true, "log".into(), Box::new(PacketLogger {
		is_client: data.is_client,
		incoming: true,
		phantom_data: PhantomData,
	}));
	data.add_packet_observer(false, "log".into(), Box::new(PacketLogger {
		is_client: data.is_client,
		incoming: false,
		phantom_data: PhantomData,
	}));
}

pub fn add_command_logger<CM: ConnectionManager + 'static>(data: &mut Data<CM>) {
	data.add_packet_observer(true, "cmdlog".into(), Box::new(CommandLogger {
		is_client: data.is_client,
		incoming: true,
		phantom_data: PhantomData,
	}));
	data.add_packet_observer(false, "cmdlog".into(), Box::new(CommandLogger {
		is_client: data.is_client,
		incoming: false,
		phantom_data: PhantomData,
	}));
}
