use std::net::SocketAddr;

use anyhow::{bail, Result};
use clap::Parser;
use tokio::time::{self, Duration};
use tracing::info;
use tsproto_packets::packets::*;

mod utils;
use crate::utils::*;

#[derive(Parser, Debug)]
#[command(author, about)]
struct Args {
	/// The address of the server to connect to
	#[arg(short, long, default_value = "127.0.0.1:9987")]
	address: SocketAddr,
	/// The listening address of the client
	#[arg(long, default_value = "0.0.0.0:0")]
	local_address: SocketAddr,
	/// Print the content of all packets
	///
	/// 0. Print nothing
	/// 1. Print command string
	/// 2. Print packets
	/// 3. Print udp packets
	#[arg(short, long, action = clap::ArgAction::Count)]
	verbose: u8,
}

#[tokio::main]
async fn main() -> Result<()> { real_main().await }

async fn real_main() -> Result<()> {
	// Parse command line options
	let args = Args::parse();
	create_logger();

	let mut con = create_client(args.local_address, args.address, args.verbose).await?;

	// Connect
	connect(&mut con).await?;
	info!("Connected");

	// Wait some time
	tokio::select! {
		_ = time::sleep(Duration::from_secs(2)) => {}
		_ = con.wait_disconnect() => {
			bail!("Disconnected");
		}
	};
	info!("Waited");

	// Send packet
	let mut cmd =
		OutCommand::new(Direction::C2S, Flags::empty(), PacketType::Command, "sendtextmessage");
	cmd.write_arg("targetmode", &3);
	cmd.write_arg("msg", &"Hello");
	let id = con.send_packet(cmd.into_packet())?;
	con.wait_for_ack(id).await?;

	// Disconnect
	disconnect(&mut con).await?;
	info!("Disconnected");

	Ok(())
}
