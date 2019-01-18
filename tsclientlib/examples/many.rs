use std::time::{Duration, Instant};

use failure::format_err;
use futures::{stream, Future, Stream};
use slog::{o, Drain};
use structopt::clap::AppSettings;
use structopt::StructOpt;
use tokio::timer::Delay;

use tsclientlib::{ConnectOptions, Connection};

#[derive(StructOpt, Debug)]
#[structopt(raw(global_settings = "&[AppSettings::ColoredHelp, \
                                   AppSettings::VersionlessSubcommands]"))]
struct Args {
	#[structopt(
		short = "a",
		long = "address",
		default_value = "localhost",
		help = "The address of the server to connect to"
	)]
	address: String,
	#[structopt(
		short = "v",
		long = "verbose",
		help = "Print the content of all packets",
		parse(from_occurrences)
	)]
	verbose: u8,
	// 0. Print nothing
	// 1. Print command string
	// 2. Print packets
	// 3. Print udp packets
	#[structopt(help = "How many connections")]
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
				let con_config = con_config.private_key_str(
					"MG0DAgeAAgEgAiAIXJBlj1hQbaH0Eq0DuLlCmH8bl+veTAO2+\
					k9EQjEYSgIgNnImcmKo7ls5mExb6skfK2Tw+u54aeDr0OP1ITs\
					C/50CIA8M5nmDBnmDM/gZ//4AAAAAAAAAAAAAAAAAAAAZRzOI").unwrap();

				// Connect
				Connection::new(con_config)
			})).collect()
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
