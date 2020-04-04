use anyhow::Result;
use futures::prelude::*;
use structopt::StructOpt;
use tokio::time::{self, Duration};

use tsclientlib::sync::SyncConnection;
use tsclientlib::{ConnectOptions, Connection, DisconnectOptions, Identity};

#[derive(StructOpt, Debug)]
#[structopt(author, about)]
struct Args {
	/// The address of the server to connect to
	#[structopt(short = "a", long, default_value = "localhost")]
	address: String,
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
async fn main() -> Result<()> { real_main().await }

async fn real_main() -> Result<()> {
	// Parse command line options
	let args = Args::from_args();

	let con_config = ConnectOptions::new(args.address)
		.log_commands(args.verbose >= 1)
		.log_packets(args.verbose >= 2)
		.log_udp_packets(args.verbose >= 3);

	// Optionally set the key of this client, otherwise a new key is generated.
	let id = Identity::new_from_str(
		"MG0DAgeAAgEgAiAIXJBlj1hQbaH0Eq0DuLlCmH8bl+veTAO2+\
		k9EQjEYSgIgNnImcmKo7ls5mExb6skfK2Tw+u54aeDr0OP1ITs\
		C/50CIA8M5nmDBnmDM/gZ//4AAAAAAAAAAAAAAAAAAAAZRzOI").unwrap();
	let con_config = con_config.identity(id);

	// Connect
	let con = Connection::new(con_config)?;
	let con: SyncConnection = con.into();
	let mut handle = con.get_handle();

	// Do event handling in another thread
	tokio::spawn(con.for_each(|_| future::ready(())));

	handle.wait_until_connected().await?;

	let welcome_message = handle
		.with_connection(|con| {
			Result::<_>::Ok(con.get_state()?.server.welcome_message.clone())
		})
		.await??;
	println!("Server welcome message: {}", sanitize(&welcome_message));

	// Wait some time
	time::delay_for(Duration::from_secs(1)).await;

	// Disconnect
	handle.disconnect(DisconnectOptions::new()).await?;

	Ok(())
}

/// Only retain a certain set of characters.
fn sanitize(s: &str) -> String {
	s.chars()
		.filter(|c| {
			c.is_alphanumeric()
				|| [
					' ', '\t', '.', ':', '-', '_', '"', '\'', '/', '(', ')',
					'[', ']', '{', '}',
				]
				.contains(c)
		})
		.collect()
}
