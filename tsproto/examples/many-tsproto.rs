use std::net::SocketAddr;

use anyhow::Result;
use clap::Parser;
use futures::prelude::*;
use tokio::time::{self, Duration};
use tracing::info;

mod utils;
use crate::utils::*;

#[derive(Parser, Clone, Debug)]
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
	/// How many connections
	#[arg()]
	count: usize,
}

#[tokio::main]
async fn main() -> Result<()> { real_main().await }

async fn real_main() -> Result<()> {
	// Parse command line options
	let args = Args::parse();
	create_logger();

	stream::iter(0..args.count)
		.for_each_concurrent(None, |_| {
			let args = args.clone();
			tokio::spawn(async move {
				let mut con =
					create_client(args.local_address, args.address, args.verbose).await.unwrap();

				// Connect
				connect(&mut con).await.unwrap();
				info!("Connected");

				// Wait some time
				tokio::select! {
					_ = time::sleep(Duration::from_secs(2)) => {}
					_ = con.wait_disconnect() => {
						panic!("Disconnected");
					}
				};
				info!("Waited");

				// Disconnect
				let _ = disconnect(&mut con).await;
				info!("Disconnected");
			})
			.map(|_| ())
		})
		.await;

	Ok(())
}
