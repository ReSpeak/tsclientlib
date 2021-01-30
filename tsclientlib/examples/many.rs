use anyhow::Result;
use futures::prelude::*;
use slog::{error, o, Drain, Logger};
use structopt::StructOpt;
use tokio::time::{self, Duration};

use tsclientlib::{Connection, DisconnectOptions, Identity, StreamItem};

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
	/// How many connections
	#[structopt()]
	count: usize,
}

#[tokio::main]
async fn main() -> Result<()> { real_main().await }

async fn real_main() -> Result<()> {
	// Parse command line options
	let args = Args::from_args();

	let logger = {
		let decorator = slog_term::TermDecorator::new().build();
		let drain = slog_term::CompactFormat::new(decorator).build().fuse();
		let drain = slog_async::Async::new(drain).build().fuse();

		Logger::root(drain, o!())
	};

	let con_config = Connection::build(args.address.as_str())
		.logger(logger.clone())
		.log_commands(args.verbose >= 1)
		.log_packets(args.verbose >= 2)
		.log_udp_packets(args.verbose >= 3);

	// Optionally set the key of this client, otherwise a new key is generated.
	let id = Identity::new_from_str(
		"MG0DAgeAAgEgAiAIXJBlj1hQbaH0Eq0DuLlCmH8bl+veTAO2+\
		k9EQjEYSgIgNnImcmKo7ls5mExb6skfK2Tw+u54aeDr0OP1ITs\
		C/50CIA8M5nmDBnmDM/gZ//4AAAAAAAAAAAAAAAAAAAAZRzOI").unwrap();
	let con_config = con_config.identity(id);

	stream::iter(0..args.count)
		.for_each_concurrent(None, |_| {
			let con_config = con_config.clone();
			let logger = logger.clone();
			tokio::spawn(async move {
				// Connect
				let mut con = con_config.connect().unwrap();

				let r = con
					.events()
					.try_filter(|e| future::ready(matches!(e, StreamItem::BookEvents(_))))
					.next()
					.await;
				if let Some(Err(e)) = r {
					error!(logger, "Connection failed"; "error" => %e);
					return;
				}

				// Wait some time
				let mut events = con.events().try_filter(|_| future::ready(false));
				tokio::select! {
					_ = time::sleep(Duration::from_secs(15)) => {}
					_ = events.next() => {
						error!(logger, "Disconnected unexpectedly");
						return;
					}
				};
				drop(events);

				// Disconnect
				let _ = con.disconnect(DisconnectOptions::new());
				con.events().for_each(|_| future::ready(())).await;
			})
			.map(|_| ())
		})
		.await;

	Ok(())
}
