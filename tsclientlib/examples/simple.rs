use std::time::{Duration, Instant};

use failure::format_err;
use futures::Future;
use structopt::clap::AppSettings;
use structopt::StructOpt;
use tokio::timer::Delay;

use tsclientlib::{ConnectOptions, Connection, Identity};

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
}

fn main() -> Result<(), failure::Error> {
	// Parse command line options
	let args = Args::from_args();

	tokio::run(
		futures::lazy(|| {
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
			Connection::new(con_config)
		})
		.and_then(|con| {
			{
				let con = con.lock();
				println!(
					"Server welcome message: {}",
					sanitize(&con.server.welcome_message)
				);
			}

			// Wait some time
			Delay::new(Instant::now() + Duration::from_secs(1))
				.map(move |_| con)
				.map_err(|e| format_err!("Failed to wait ({:?})", e).into())
		})
		.and_then(|con| {
			// Disconnect
			drop(con);
			Ok(())
		})
		.map_err(|e| panic!("An error occurred {:?}", e)),
	);

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
