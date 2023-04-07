use anyhow::Result;
use clap::Parser;
use futures::prelude::*;
use tokio::time::{self, Duration};

use tsclientlib::prelude::*;
use tsclientlib::sync::SyncConnection;
use tsclientlib::{Connection, DisconnectOptions, Identity, StreamItem};

#[derive(Parser, Debug)]
#[command(author, about)]
struct Args {
	/// The address of the server to connect to
	#[arg(short, long, default_value = "localhost")]
	address: String,
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
	tracing_subscriber::fmt::init();

	// Parse command line options
	let args = Args::parse();

	let con_config = Connection::build(args.address)
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
	let mut con = con_config.connect()?;
	let r = con
		.events()
		.try_filter(|e| future::ready(matches!(e, StreamItem::BookEvents(_))))
		.next()
		.await;
	if let Some(r) = r {
		r?;
	}

	let sync_con: SyncConnection = con.into();
	let mut con = sync_con.get_handle();

	tokio::spawn(sync_con.for_each(|_| future::ready(())));

	con.with_connection(move |mut con| {
		let state = con.get_state()?;
		state.server.send_textmessage("Hello there").send(&mut con)?;
		Result::<_>::Ok(())
	})
	.await??;

	// Wait some time
	time::sleep(Duration::from_secs(1)).await;

	// Disconnect
	con.disconnect(DisconnectOptions::new()).await?;

	Ok(())
}
