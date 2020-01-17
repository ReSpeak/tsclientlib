use std::net::SocketAddr;
use std::time::{Duration, Instant};

use futures::{future, stream, Future, Stream};
use slog::{info, o, Drain};
use structopt::StructOpt;
use tokio::timer::Delay;

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
	/// How many connections
	#[structopt()]
	count: usize,
}

fn main() {
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
			stream::iter_ok(0..args.count)
				.map(move |_| {
					let c = create_client(
						args.local_address.clone(),
						logger.clone(),
						SimplePacketHandler,
						args.verbose,
					);

					// Connect
					let logger = logger.clone();
					let logger2 = logger.clone();
					let logger3 = logger.clone();
					let c2 = c.clone();
					connect(logger.clone(), c.clone(), args.address)
				.and_then(move |con| {
					info!(logger2, "Connected");
					// Wait some time
					Delay::new(Instant::now() + Duration::from_secs(5))
						.map(move |_| con)
						.map_err(|e| e.into())
				})
				/*.and_then(move |con| {
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
					con.as_packet_sink()
						.send(packet)
						.map_err(|e| panic!("Failed to send packet ({:?})", e))
						.and_then(|_| {
							Delay::new(Instant::now() + Duration::from_secs(3))
						})
						.map(move |_| con)
				})*/
				.and_then(move |con| {
					// Disconnect
					disconnect(&c2, con).map_err(|e| {
						panic!("Failed to disconnect ({:?})", e)
					}).map(move |_| c2)
				})
				.and_then(move |c| {
					info!(logger3, "Disconnected");
					// Quit client
					drop(c);
					Ok(())
				})
				})
				.buffered(10000)
				.for_each(|_| Ok(()))
		})
		.map_err(|e| println!("An error occurred {:?}", e)),
	);
}
