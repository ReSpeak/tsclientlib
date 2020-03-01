use failure::format_err;
use futures::future;
use futures::prelude::*;
use slog::{error, o, Drain, Logger};
use structopt::StructOpt;
use tokio::runtime::current_thread::Runtime;
use tokio::sync::mpsc;
use tsproto_packets::packets::{InAudio, InCommand};

use tsclientlib::{ConnectOptions, Connection, Identity, PHBox, PacketHandler};

mod audio_utils;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct ConnectionId(u64);

#[derive(StructOpt, Debug)]
#[structopt(author, about)]
struct Args {
	/// The address of the server to connect to
	#[structopt(short = "a", long, default_value = "localhost")]
	address: String,
	/// The volume for the capturing
	#[structopt(default_value = "1.0")]
	volume: f32,
	/// Print the content of all packets
	///
	/// 0. Print nothing
	/// 1. Print command string
	/// 2. Print packets
	/// 3. Print udp packets
	#[structopt(short = "v", long, parse(from_occurrences))]
	verbose: u8,
}

#[derive(Clone)]
struct AudioPacketHandler {
	logger: Logger,
	con: ConnectionId,
	send: mpsc::Sender<(ConnectionId, InAudio)>,
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

	// We need an explicit runtime and executor because we want to spawn new
	// tasks in callbacks from other threads.
	let mut runtime = Runtime::new().unwrap();
	let executor = runtime.handle();
	let executor2 = executor.clone();

	let con_id = ConnectionId(0);
	let vol = args.volume;
	let logger2 = logger.clone();
	runtime.block_on(
			futures::lazy(move || -> Box<dyn Future<Item=_, Error=_>> {
				let audiodata = match audio_utils::start(logger, executor2) {
					Ok(r) => r,
					Err(e) => return Box::new(future::err(e.into())),
				};

				let (send, recv) = mpsc::channel(5);

				let logger = logger2.clone();
				let con_config = ConnectOptions::new(args.address)
					.log_commands(args.verbose >= 1)
					.log_packets(args.verbose >= 2)
					.log_udp_packets(args.verbose >= 3)
					.handle_packets(Box::new(AudioPacketHandler {
						logger,
						con: con_id,
						send,
					}));

				let t2a = audiodata.ts2a.clone();
				tokio::runtime::current_thread::spawn(recv
					.map_err(|e| e.into())
					.for_each(move |(con, packet)| {
						let mut t2a = t2a.lock().unwrap();
						t2a.play_packet(con, &packet)
					})
					.map_err(move |e| error!(logger2,
						"Failed to redirect audio packet"; "error" => ?e)));

				// Optionally set the key of this client, otherwise a new key is generated.
				let id = Identity::new_from_str(
					"MG0DAgeAAgEgAiAIXJBlj1hQbaH0Eq0DuLlCmH8bl+veTAO2+\
					k9EQjEYSgIgNnImcmKo7ls5mExb6skfK2Tw+u54aeDr0OP1ITs\
					C/50CIA8M5nmDBnmDM/gZ//4AAAAAAAAAAAAAAAAAAAAZRzOI").unwrap();
				let con_config = con_config.identity(id);

				// Connect
				Box::new(Connection::new(con_config).map(|c| (c, audiodata)))
			})
			.and_then(move |(con, audiodata)| -> Box<dyn Future<Item=_, Error=_>> {
				{
					let mut a2t = audiodata.a2ts.lock().unwrap();
					a2t.set_listener(&con);
					a2t.set_volume(vol);
					a2t.set_playing(true);
				}

				// Wait for ctrl + c
				let ctrl_c = tokio_signal::ctrl_c().flatten_stream();
				Box::new(ctrl_c
					.into_future()
					.map_err(|_| {
						format_err!("Failed to wait for ctrl + c").into()
					})
					.map(move |_| (con, audiodata)))
			})
			.and_then(|(con, _)| {
				// Disconnect
				drop(con);
				Ok(())
			})
			.map_err(|e| panic!("An error occurred {:?}", e)),
		).unwrap();

	// Not everything is cleanup perfectly
	//runtime.run().unwrap();

	Ok(())
}

impl PacketHandler for AudioPacketHandler {
	fn new_connection(
		&mut self,
		command_stream: Box<
			dyn Stream<Item = InCommand, Error = tsproto::Error> + Send,
		>,
		audio_stream: Box<
			dyn Stream<Item = InAudio, Error = tsproto::Error> + Send,
		>,
	)
	{
		let logger = self.logger.clone();
		tokio::runtime::current_thread::spawn(
			command_stream.for_each(|_| Ok(())).then(move |r| {
				if let Err(e) = r {
					error!(logger, "Failed to handle packets"; "error" => ?e);
				}
				Ok(())
			}),
		);

		let logger = self.logger.clone();
		let con = self.con;
		let mut send = self.send.clone();
		tokio::runtime::current_thread::spawn(
			audio_stream
				.for_each(move |packet| {
					send.try_send((con, packet)).unwrap();
					Ok(())
				})
				.map_err(
					move |e| error!(logger, "Failed to handle packets"; "error" => ?e),
				),
		);
	}

	/// Clone into a box.
	fn clone(&self) -> PHBox { Box::new(Clone::clone(self)) }
}
