use std::net::SocketAddr;
use std::sync::Mutex;

use anyhow::Result;
use tokio::net::UdpSocket;
use tsproto::algorithms as algs;
use tsproto::client::Client;
use tsproto::crypto::EccKeyPrivP256;
use tsproto_packets::packets::*;

#[allow(dead_code)]
pub fn create_logger(to_file: bool) {
	if to_file {
		tracing_subscriber::fmt().with_writer(std::fs::File::create("bench.log").unwrap())::init();
	} else {
		tracing_subscriber::fmt::init();
	}
}

pub async fn create_client(
	local_address: SocketAddr, remote_address: SocketAddr, verbose: u8,
) -> Result<Client> {
	// Get P-256 ECDH key
	let private_key = EccKeyPrivP256::import_str(
		"MG0DAgeAAgEgAiAIXJBlj1hQbaH0Eq0DuLlCmH8bl+veTAO2+\
		k9EQjEYSgIgNnImcmKo7ls5mExb6skfK2Tw+u54aeDr0OP1ITsC/50CIA8M5nm\
		DBnmDM/gZ//4AAAAAAAAAAAAAAAAAAAAZRzOI").unwrap();

	let udp_socket = UdpSocket::bind(local_address).await?;
	let mut con = Client::new(remote_address, Box::new(udp_socket), private_key);

	if verbose >= 1 {
		tsproto::log::add_logger(verbose - 1, &mut con)
	}

	Ok(con)
}

/// Returns the `initserver` command.
pub async fn connect(con: &mut Client) -> Result<InCommandBuf> {
	con.connect().await?;

	// Send clientinit
	let private_key = EccKeyPrivP256::import_str(
		"MG0DAgeAAgEgAiAIXJBlj1hQbaH0Eq0DuLlCmH8bl+veTAO2+\
		k9EQjEYSgIgNnImcmKo7ls5mExb6skfK2Tw+u54aeDr0OP1ITsC/50CIA8M5nm\
		DBnmDM/gZ//4AAAAAAAAAAAAAAAAAAAAZRzOI").unwrap();

	let private_key_as_pub = private_key.to_pub();
	let offset = algs::hash_cash(&private_key_as_pub, 8).unwrap();

	// Create clientinit packet
	let offset = offset.to_string();
	let packet = OutCommand::new::<_, _, String, String, _, _, std::iter::Empty<_>>(
		Direction::C2S,
		PacketType::Command,
		"clientinit",
		vec![
			("client_nickname", "Bot"),
			("client_version", "3.?.? [Build: 5680278000]"),
			("client_platform", "Linux"),
			("client_input_hardware", "1"),
			("client_output_hardware", "1"),
			("client_default_channel", ""),
			("client_default_channel_password", ""),
			("client_server_password", ""),
			("client_meta_data", ""),
			(
				"client_version_sign",
				"Hjd+N58Gv3ENhoKmGYy2bNRBsNNgm5kpiaQWxOj5HN2DXttG6REjymSwJtpJ8muC2gSwRuZi0R+8Laan5ts5CQ==",
			),
			("client_nickname_phonetic", ""),
			("client_key_offset", &offset),
			("client_default_token", ""),
			("client_badges", "Overwolf=0"),
			(
				"hwid",
				"923f136fb1e22ae6ce95e60255529c00,\
				 d13231b1bc33edfecfb9169cc7a63bcc",
			),
		]
		.into_iter(),
		std::iter::empty(),
	);

	con.send_packet(packet)?;
	Ok(con
		.filter_commands(|con, cmd| {
			Ok(if cmd.data().packet().content().starts_with(b"initserver ") {
				Some(cmd)
			} else {
				con.hand_back_buffer(cmd.into_buffer());
				None
			})
		})
		.await?)
}

pub async fn disconnect(con: &mut Client) -> Result<()> {
	let packet = OutCommand::new::<_, _, String, String, _, _, std::iter::Empty<_>>(
		Direction::C2S,
		PacketType::Command,
		"clientdisconnect",
		vec![
			// Reason: Disconnect
			("reasonid", "8"),
			("reasonmsg", "Bye"),
		]
		.into_iter(),
		std::iter::empty(),
	);

	con.send_packet(packet)?;
	con.wait_disconnect().await?;
	Ok(())
}
