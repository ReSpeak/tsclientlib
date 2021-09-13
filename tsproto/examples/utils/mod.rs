use std::net::SocketAddr;

use anyhow::Result;
use tokio::net::UdpSocket;
use tracing::{info, info_span};
use tsproto::algorithms as algs;
use tsproto::client::Client;
use tsproto_packets::packets::*;
use tsproto_types::crypto::EccKeyPrivP256;

pub fn create_logger() { tracing_subscriber::fmt::init(); }

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
		tsproto::log::add_logger_with_verbosity(verbose, &mut con)
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

	// Compute hash cash
	let offset;
	{
		let _span = info_span!("Compute public key hash cash level").entered();
		let private_key_as_pub = private_key.to_pub();
		offset = algs::hash_cash(&private_key_as_pub, 8);
		let omega = private_key_as_pub.to_ts();
		info!(
			level = algs::get_hash_cash_level(&omega, offset),
			offset, "Computed hash cash level"
		);
	}

	// Create clientinit packet
	let offset = offset.to_string();
	let mut cmd =
		OutCommand::new(Direction::C2S, Flags::empty(), PacketType::Command, "clientinit");
	cmd.write_arg("client_nickname", &"Bot");
	cmd.write_arg("client_version", &"3.?.? [Build: 5680278000]");
	cmd.write_arg("client_platform", &"Linux");

	cmd.write_arg("client_input_hardware", &"1");
	cmd.write_arg("client_output_hardware", &"1");
	cmd.write_arg("client_default_channel", &"");
	cmd.write_arg("client_default_channel_password", &"");
	cmd.write_arg("client_server_password", &"");
	cmd.write_arg("client_meta_data", &"");
	cmd.write_arg(
		"client_version_sign",
		&"Hjd+N58Gv3ENhoKmGYy2bNRBsNNgm5kpiaQWxOj5HN2DXttG6REjymSwJtpJ8muC2gSwRuZi0R+8Laan5ts5CQ==",
	);
	cmd.write_arg("client_nickname_phonetic", &"");
	cmd.write_arg("client_key_offset", &offset);
	cmd.write_arg("client_default_token", &"");
	cmd.write_arg("client_badges", &"Overwolf=0");
	cmd.write_arg("hwid", &"923f136fb1e22ae6ce95e60255529c00,d13231b1bc33edfecfb9169cc7a63bcc");

	con.send_packet(cmd.into_packet())?;
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
	let mut cmd =
		OutCommand::new(Direction::C2S, Flags::empty(), PacketType::Command, "clientdisconnect");
	cmd.write_arg("reasonid", &8);
	cmd.write_arg("reasonmsg", &"Bye");

	con.send_packet(cmd.into_packet())?;
	con.wait_disconnect().await?;
	Ok(())
}
