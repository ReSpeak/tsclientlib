extern crate base64;
extern crate cpuprofiler;
extern crate failure;
extern crate futures;
extern crate gstreamer as gst;
extern crate gstreamer_app as gst_app;
extern crate gstreamer_audio as gst_audio;
#[macro_use]
extern crate slog;
extern crate slog_async;
extern crate slog_perf;
extern crate slog_term;
extern crate structopt;
extern crate tokio;
extern crate tsproto;

use std::net::SocketAddr;
use std::time::Duration;
use std::time::Instant;

use futures::{future, stream, Future, Sink, Stream};
use slog::Drain;
use structopt::clap::AppSettings;
use structopt::StructOpt;
use tokio::timer::Delay;
use tsproto::packets::*;
use tsproto::*;

mod utils;
use utils::*;

#[derive(StructOpt, Debug)]
#[structopt(raw(
	global_settings = "&[AppSettings::ColoredHelp, \
	                   AppSettings::VersionlessSubcommands]"
))]
struct Args {
	#[structopt(
		short = "a",
		long = "address",
		default_value = "127.0.0.1:9987",
		help = "The address of the server to connect to"
	)]
	address: SocketAddr,
	#[structopt(
		long = "local-address",
		default_value = "0.0.0.0:0",
		help = "The listening address of the client"
	)]
	local_address: SocketAddr,
	#[structopt(
		short = "n",
		long = "amount",
		default_value = "100",
		help = "The amount of packets to send"
	)]
	amount: u32,
	#[structopt(
		short = "v",
		long = "verbose",
		help = "Display the content of all packets"
	)]
	verbose: bool,
}

fn main() {
	tsproto::init().unwrap();

	// Parse command line options
	let args = Args::from_args();

	let logger = {
		let decorator = slog_term::TermDecorator::new().build();
		let drain = slog_term::CompactFormat::new(decorator).build().fuse();
		let drain = slog_async::Async::new(drain).build().fuse();

		slog::Logger::root(drain, o!())
	};

	tokio::run(
		future::lazy(move || {
			let c = create_client(
				args.local_address,
				logger.clone(),
				SimplePacketHandler,
				args.verbose,
			);

			// Connect
			let logger2 = logger.clone();
			connect(logger.clone(), c.clone(), args.address)
				.map_err(|e| panic!("Failed to connect ({:?})", e))
				.and_then(move |con| {
					info!(logger2, "Connected");
					Delay::new(Instant::now() + Duration::from_secs(2))
						.map(|_| con)
						.map_err(|e| panic!("An error occurred {:?}", e))
				}).and_then(move |con| {
					info!(logger, "Waited");
					// Send packet
					let mut header = Header::default();
					header.set_type(PacketType::Command);
					let mut cmd = commands::Command::new("sendtextmessage");
					cmd.push("targetmode", "3");

					// Benchmark sending messages
					let count = args.amount;
					let mut time_reporter =
						slog_perf::TimeReporter::new_with_level(
							"Message benchmark",
							logger.clone(),
							slog::Level::Info,
						);
					time_reporter.start("Messages");
					let start = Instant::now();

					//cpuprofiler::PROFILER.lock().unwrap().start("./message-bench.profile").unwrap();
					//cpuprofiler::PROFILER.lock().unwrap().stop().unwrap();
					let con2 = con.clone();
					stream::iter_ok(0..count)
						.and_then(move |i| {
							let mut cmd = cmd.clone();
							cmd.push("msg", format!("Hello {}", i));
							let packet =
								Packet::new(header.clone(), Data::Command(cmd));
							con.as_packet_sink().send(packet)
						}).for_each(|_| future::ok(()))
						.and_then(move |_| {
							time_reporter.finish();
							let dur = start.elapsed();

							info!(
								logger,
								"{} messages in {}.{:03}s",
								count,
								dur.as_secs(),
								dur.subsec_nanos() / 1_000_000,
							);
							let dur = dur / count;
							info!(
								logger,
								"{}.{:03}ms per message",
								dur.subsec_nanos() / 1_000_000,
								dur.subsec_nanos() / 1_000,
							);

							// Wait some time
							Delay::new(Instant::now() + Duration::from_secs(1))
								.map_err(|e| {
									panic!("An error occurred {:?}", e)
								}).and_then(move |_| {
									disconnect(con2).map_err(|e| {
										panic!("Failed to disconnect ({:?})", e)
									})
								}).and_then(move |_| {
									info!(logger, "Disconnected");
									drop(c);
									Ok(())
								})
						})
				})
		}).map_err(|e| panic!("An error occurred {:?}", e)),
	);
}
