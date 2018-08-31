extern crate base64;
extern crate futures;
extern crate ring;
#[macro_use]
extern crate slog;
extern crate slog_async;
extern crate slog_perf;
extern crate slog_term;
extern crate structopt;
extern crate tokio_core;
extern crate tsproto;

use std::net::SocketAddr;
use std::rc::Rc;
use std::time::Duration;

use futures::Sink;
use slog::Drain;
use structopt::StructOpt;
use structopt::clap::AppSettings;
use tokio_core::reactor::{Core, Timeout};
use tsproto::*;
use tsproto::packets::*;

mod utils;
use utils::*;

#[derive(StructOpt, Debug)]
#[structopt(raw(global_settings =
    "&[AppSettings::ColoredHelp, AppSettings::VersionlessSubcommands]"))]
struct Args {
    #[structopt(short = "a", long = "address",
                default_value = "127.0.0.1:9987",
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
        // Or FullFormat
        let drain = slog_term::CompactFormat::new(decorator).build().fuse();
        let drain = slog_async::Async::new(drain).build().fuse();

        slog::Logger::root(drain, o!())
    };

    let c = create_client(args.local_address, core.handle(), logger.clone(), true);

    // Connect
    let handle = core.handle();
    if let Err(error) = core.run(connect(logger.clone(), &handle, c.clone(),
        args.address)) {
        error!(logger, "Failed to connect"; "error" => ?error);
        return;
    }
    info!(logger, "Connected");

    // Wait some time
    let action = Timeout::new(Duration::from_secs(2), &core.handle()).unwrap();
    core.run(action).unwrap();
    info!(logger, "Waited");

    // Send packet
    let mut header = Header::default();
    header.set_type(PacketType::Command);
    let mut cmd = commands::Command::new("sendtextmessage");

    cmd.push("targetmode", "3");
    cmd.push("msg", "Hello");

    let packets = handler_data::Data::get_packets(Rc::downgrade(&c));
    let packet = Packet::new(header, Data::Command(cmd));
    core.run(packets.send((args.address, packet.clone()))).unwrap();

    // Wait some time
    let action = Timeout::new(Duration::from_secs(3), &core.handle()).unwrap();
    core.run(action).unwrap();

    // Disconnect
    if let Err(error) = core.run(disconnect(c.clone(),
        args.address)) {
        error!(logger, "Failed to disconnect"; "error" => ?error);
        return;
    }
    info!(logger, "Disconnected");
}
