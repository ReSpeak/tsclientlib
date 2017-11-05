extern crate futures;
#[macro_use]
extern crate slog;
extern crate slog_async;
extern crate slog_term;
extern crate structopt;
#[macro_use]
extern crate structopt_derive;
extern crate tokio_core;
extern crate tsproto;

use std::net::SocketAddr;
use std::time::Duration;

use futures::{future, Sink, Stream, Future};
use slog::Drain;
use structopt::StructOpt;
use structopt::clap::AppSettings;
use tokio_core::reactor::{Core, Timeout};
use tsproto::*;
use tsproto::packets::*;

#[derive(StructOpt, Debug)]
#[structopt(global_settings_raw = "&[AppSettings::ColoredHelp, AppSettings::VersionlessSubcommands]")]
struct Args {
	#[structopt(short = "a", long = "address", default_value = "127.0.0.1:9987",
		help = "The address of the server to connect to")]
	address: SocketAddr,
	#[structopt(long = "local-address", default_value = "0.0.0.0:0",
		help = "The listening address of the client")]
	local_address: SocketAddr,
}

fn main() {
	tsproto::init().unwrap();

	// Parse command line options
	let args = Args::from_args();
	let mut core = Core::new().unwrap();

	let logger = {
		let decorator = slog_term::TermDecorator::new().build();
		let drain = slog_term::FullFormat::new(decorator).build().fuse();
		let drain = slog_async::Async::new(drain).build().fuse();

		slog::Logger::root(drain, o!())
	};

	let c = client::ClientData::new(args.local_address, core.handle(), true, logger.clone()).unwrap();
	client::default_setup(c.clone());

	// Listen for packets
	let listen = client::ClientData::get_packets(c.clone())
		.for_each(|_| future::ok(()))
		.map(|()| println!("Listening finished"))
		.map_err(|error| println!("Listening error: {:?}", error));
	core.handle().spawn(listen);

	// Connect
	core.run(client::connect(c.clone(), args.address)).unwrap();
	info!(logger, "Connected");

	// Wait some time
	let action = Timeout::new(Duration::from_secs(5), &core.handle()).unwrap();
	core.run(action).unwrap();
	info!(logger, "Waited");

	// Send packet
	let sink = client::ClientData::get_packets(c.clone());
	let mut header = Header::default();
	header.set_type(PacketType::Command);
	let mut cmd = commands::Command::new("sendtextmessage");

	cmd.push("targetmode", "3");
	cmd.push("msg", "Hello");

	let packet = Packet::new(header, Data::Command(cmd));
	core.run(sink.send((args.address, packet))).unwrap();

	// Wait some time
	let action = Timeout::new(Duration::from_secs(3), &core.handle()).unwrap();
	core.run(action).unwrap();

	// Disconnect
	core.run(client::disconnect(c.clone(), args.address)).unwrap();
	info!(logger, "Disconnected");

	// Wait some time
	let action = Timeout::new(Duration::from_secs(3), &core.handle()).unwrap();
	core.run(action).unwrap();
}
