use failure::format_err;
use futures::{Future, Stream};
use slog::{o, Drain};
use structopt::clap::AppSettings;
use structopt::StructOpt;
use tokio::runtime::Runtime;
#[cfg(feature = "audio")]
use tsproto_audio::{audio_to_ts, ts_to_audio};

use tsclientlib::{ConnectOptions, Connection, ConnectionPacketSinkCreator};

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
		short = "u",
		long = "uri",
		help = "Play an uri instead of sending microphone input"
	)]
	uri: Option<String>,
	#[structopt(
		long = "volume",
		default_value = "0.0",
		help = "The volume for the capturing"
	)]
	volume: f64,
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

#[cfg(not(feature = "audio"))]
fn main() {
	eprintln!("This example can only be run with the 'audio' feature.");
	eprintln!("Run with `cargo run --features audio --example audio`.");
	std::process::exit(1);
}

#[cfg(feature = "audio")]
fn main() -> Result<(), failure::Error> {
	// Parse command line options
	let args = Args::from_args();

	let logger = {
		let decorator = slog_term::TermDecorator::new().build();
		let drain = slog_term::CompactFormat::new(decorator).build().fuse();
		let drain = slog_async::Async::new(drain).build().fuse();

		slog::Logger::root(drain, o!())
	};

	// We need an explicit runtime and executor because we want to spawn new
	// tasks in callbacks from gstreamer threads.
	let runtime = Runtime::new().unwrap();
	let executor = runtime.executor();
	let executor2 = executor.clone();

	let uri = args.uri.clone();
	let vol = args.volume;
	let logger2 = logger.clone();
	runtime
		.block_on_all(
			futures::lazy(move || {
				let t2a_pipe =
					ts_to_audio::Pipeline::new(logger2, executor).unwrap();
				let aph = t2a_pipe.create_packet_handler();

				let con_config = ConnectOptions::new(args.address)
					.log_commands(args.verbose >= 1)
					.log_packets(args.verbose >= 2)
					.log_udp_packets(args.verbose >= 3)
					.audio_packet_handler(aph);

				// Optionally set the key of this client, otherwise a new key is generated.
				let con_config = con_config.private_key_str(
				"MG0DAgeAAgEgAiAIXJBlj1hQbaH0Eq0DuLlCmH8bl+veTAO2+\
				k9EQjEYSgIgNnImcmKo7ls5mExb6skfK2Tw+u54aeDr0OP1ITs\
				C/50CIA8M5nmDBnmDM/gZ//4AAAAAAAAAAAAAAAAAAAAZRzOI").unwrap();

				// Connect
				Connection::new(con_config).map(move |con| (con, t2a_pipe))
			})
			.and_then(move |(con, t2a_pipe)| {
				let sink_creator =
					ConnectionPacketSinkCreator::new(con.clone());
				// TODO Don't unwrap, disconnect if it did not work
				let a2t_pipe = audio_to_ts::Pipeline::new(
					logger,
					sink_creator,
					executor2,
					uri.as_ref().map(|s| s.as_str()),
				)
				.unwrap();
				a2t_pipe.set_volume(vol);

				// Wait for ctrl + c
				let ctrl_c = tokio_signal::ctrl_c().flatten_stream();
				ctrl_c
					.into_future()
					.map_err(|_| {
						format_err!("Failed to wait for ctrl + c").into()
					})
					.map(move |_| (con, t2a_pipe, a2t_pipe))
			})
			.and_then(|(con, _, _)| {
				// Disconnect
				drop(con);
				Ok(())
			})
			.map_err(|e| panic!("An error occurred {:?}", e)),
		)
		.unwrap();

	Ok(())
}
