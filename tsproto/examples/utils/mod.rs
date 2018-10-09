use std::net::SocketAddr;
use std::sync::Arc;

use futures::{Future, Sink, Stream};
use tokio;
use tsproto::algorithms as algs;
use tsproto::client::ServerConnectionData;
use tsproto::crypto::EccKeyPrivP256;
use tsproto::handler_data::PacketHandler;
use tsproto::packets::*;
use tsproto::*;
use {slog, slog_perf};

pub mod voice;

pub struct SimplePacketHandler;

impl<T: 'static> PacketHandler<T> for SimplePacketHandler {
	fn new_connection<S1, S2>(
		&mut self,
		_: &handler_data::ConnectionValue<T>,
		command_stream: S1,
		audio_stream: S2,
	) where
		S1: Stream<Item = Packet, Error = Error> + Send + 'static,
		S2: Stream<Item = Packet, Error = Error> + Send + 'static,
	{
		tokio::spawn(command_stream.for_each(|_| Ok(())).map_err(|e| {
			println!("Command stream exited with error ({:?})", e)
		}));
		tokio::spawn(
			audio_stream.for_each(|_| Ok(())).map_err(|e| {
				println!("Audio stream exited with error ({:?})", e)
			}),
		);
	}
}

pub fn create_client<PH: PacketHandler<ServerConnectionData>>(
	local_address: SocketAddr,
	logger: slog::Logger,
	packet_handler: PH,
	log: bool,
) -> client::ClientDataM<PH> {
	// Get P-256 ECDH key
	let private_key = EccKeyPrivP256::from_ts(
		"MG0DAgeAAgEgAiAIXJBlj1hQbaH0Eq0DuLlCmH8bl+veTAO2+\
		k9EQjEYSgIgNnImcmKo7ls5mExb6skfK2Tw+u54aeDr0OP1ITsC/50CIA8M5nm\
		DBnmDM/gZ//4AAAAAAAAAAAAAAAAAAAAZRzOI").unwrap();

	let log_config = handler_data::LogConfig::new(log, log);
	let c = client::ClientData::new(
		local_address,
		private_key,
		true,
		None,
		client::DefaultPacketHandler::new(packet_handler),
		connectionmanager::SocketConnectionManager::new(),
		logger,
		log_config,
	).unwrap();

	// Set the data reference
	let c2 = Arc::downgrade(&c);
	c.try_lock().unwrap().packet_handler.complete(c2);

	c
}

pub fn connect<PH: PacketHandler<ServerConnectionData>>(
	logger: slog::Logger,
	client: client::ClientDataM<PH>,
	server_addr: SocketAddr,
) -> impl Future<Item = client::ClientConVal, Error = Error> {
	client::connect(Arc::downgrade(&client), &mut *client.lock().unwrap(), server_addr)
	.and_then(move |con| {
		let private_key = EccKeyPrivP256::from_ts(
			"MG0DAgeAAgEgAiAIXJBlj1hQbaH0Eq0DuLlCmH8bl+veTAO2+\
			k9EQjEYSgIgNnImcmKo7ls5mExb6skfK2Tw+u54aeDr0OP1ITsC/50CIA8M5nm\
			DBnmDM/gZ//4AAAAAAAAAAAAAAAAAAAAZRzOI").unwrap();

		// Compute hash cash
		let mut time_reporter = slog_perf::TimeReporter::new_with_level(
			"Compute public key hash cash level", logger.clone(),
			slog::Level::Info);
		time_reporter.start("Compute public key hash cash level");
		let private_key_as_pub = private_key.to_pub();
		let offset = algs::hash_cash(&private_key_as_pub, 8).unwrap();
		let omega = private_key_as_pub.to_ts().unwrap();
		time_reporter.finish();
		info!(logger, "Computed hash cash level";
			"level" => algs::get_hash_cash_level(&omega, offset),
			"offset" => offset);

		// Create clientinit packet
		let header = Header::new(PacketType::Command);
		let mut command = commands::Command::new("clientinit");
		command.push("client_nickname", "Bot");
		command.push("client_version", "3.1.8 [Build: 1516614607]");
		command.push("client_platform", "Linux");
		command.push("client_input_hardware", "1");
		command.push("client_output_hardware", "1");
		command.push("client_default_channel", "");
		command.push("client_default_channel_password", "");
		command.push("client_server_password", "");
		command.push("client_meta_data", "");
		command.push("client_version_sign", "LJ5q+KWT4KwBX7oR/9j9A12hBrq5ds5ony99f9kepNmqFskhT7gfB51bAJNgAMOzXVCeaItNmc10F2wUNktqCw==");
		command.push("client_key_offset", offset.to_string());
		command.push("client_nickname_phonetic", "");
		command.push("client_default_token", "");
		command.push("client_badges", "Overwolf=0");
		command.push("hwid", "923f136fb1e22ae6ce95e60255529c00,d13231b1bc33edfecfb9169cc7a63bcc");
		let p_data = packets::Data::Command(command);
		let clientinit_packet = Packet::new(header, p_data);

		let con2 = con.clone();
		con.as_packet_sink().send(clientinit_packet)
			.and_then(move |_| client::wait_until_connected(&con))
			.map(move |_| con2)
	})
}

pub fn disconnect(
	con: client::ClientConVal,
) -> impl Future<Item = (), Error = Error> {
	let header = Header::new(PacketType::Command);
	let mut command = commands::Command::new("clientdisconnect");

	// Reason: Disconnect
	command.push("reasonid", "8");
	command.push("reasonmsg", "Bye");
	let p_data = packets::Data::Command(command);
	let packet = Packet::new(header, p_data);

	con.as_packet_sink().send(packet).and_then(move |_| {
		client::wait_for_state(&con, |state| {
			if let client::ServerConnectionState::Disconnected = *state {
				true
			} else {
				false
			}
		})
	})
}
