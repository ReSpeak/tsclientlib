use std::time::{Duration, Instant};

use failure::format_err;
use futures::{stream, Future, Stream};
use slog::{o, Drain};
use structopt::StructOpt;
use tokio::timer::Delay;

use tsclientlib::{ConnectOptions, Connection, Identity};

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

fn main() -> Result<(), failure::Error> {
	// Parse command line options
	let args = Args::from_args();

	let logger = {
		let decorator = slog_term::TermDecorator::new().build();
		let drain = slog_term::CompactFormat::new(decorator).build().fuse();
		let drain = slog_async::Async::new(drain).build().fuse();

		slog::Logger::root(drain, o!())
	};

	tokio::run(
		futures::lazy(move || {
			stream::futures_ordered((0..args.count).map(move |i| {
				let con_config = ConnectOptions::new(args.address.as_str())
					.logger(logger.new(o!("i" => i)))
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
				Connection::new(con_config)
			}))
			.collect()
		})
		.and_then(|cons| {
			// Wait some time
			Delay::new(Instant::now() + Duration::from_secs(5))
				.map(move |_| cons)
				.map_err(|e| format_err!("Failed to wait ({:?})", e).into())
		})
		.and_then(|cons| {
			// Disconnect
			drop(cons);
			Ok(())
		})
		.map_err(|e| panic!("An error occurred {:?}", e)),
	);

	Ok(())
}
