use std::net::SocketAddr;

use anyhow::{bail, Result};
use slog::info;
use structopt::StructOpt;
use tokio::time::{self, Duration};
use tsproto_packets::packets::*;

mod utils;
use crate::utils::*;

#[derive(StructOpt, Debug)]
#[structopt(author, about)]
struct Args {
	/// The address of the server to connect to
	#[structopt(short = "a", long, default_value = "127.0.0.1:9987")]
	address: SocketAddr,
	/// The listening address of the client
	#[structopt(long, default_value = "0.0.0.0:0")]
	local_address: SocketAddr,
	/// Print the content of all packets
	///
	/// 0. Print nothing
	/// 1. Print command string
	/// 2. Print packets
	/// 3. Print udp packets
	#[structopt(short = "v", long, parse(from_occurrences))]
	verbose: u8,
}

#[tokio::main]
async fn main() -> Result<()> {
	// Parse command line options
	let args = Args::from_args();
	let logger = create_logger();

	let mut con = create_client(
		args.local_address,
		args.address,
		logger.clone(),
		args.verbose,
	).await?;

	// Connect
	connect(&mut con).await?;
	info!(logger, "Connected");

	// Wait some time
	tokio::select! {
		_ = &mut time::delay_for(Duration::from_secs(2)) => {}
		_ = wait_disconnect(&mut con) => {
			bail!("Disconnected");
		}
	};
	info!(logger, "Waited");

	// Send packet
	let packet = OutCommand::new::<
		_,
		_,
		String,
		String,
		_,
		_,
		std::iter::Empty<_>,
	>(
		Direction::C2S,
		PacketType::Command,
		"sendtextmessage",
		vec![("targetmode", "3"), ("msg", "Hello")].into_iter(),
		std::iter::empty(),
	);
	let mut fut = con.send_packet_with_answer(packet).await;
	tokio::select! {
		_ = &mut fut => {}
		_ = wait_disconnect(&mut con) => {
			bail!("Disconnected");
		}
	};

	// Disconnect
	disconnect(&mut con).await?;
	info!(logger, "Disconnected");

	Ok(())
}
