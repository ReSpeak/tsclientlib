extern crate base64;
extern crate cpuprofiler;
extern crate futures;
#[macro_use]
extern crate slog;
extern crate slog_async;
extern crate slog_perf;
extern crate slog_term;
extern crate structopt;
#[macro_use]
extern crate structopt_derive;
extern crate tokio_core;
extern crate tsproto;

use std::net::SocketAddr;
use std::rc::Rc;
use std::time::Duration;
use std::time::Instant;

use futures::{future, Sink, stream, Stream};
use slog::Drain;
use structopt::StructOpt;
use structopt::clap::AppSettings;
use tokio_core::reactor::{Core, Timeout};
use tsproto::*;
use tsproto::connectionmanager::ConnectionManager;
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
    #[structopt(short = "n", long = "amount", default_value = "100",
                help = "The amount of packets to send")]
    amount: u32,
    #[structopt(short = "v", long = "verbose",
                help = "Display the content of all packets")]
    verbose: bool,
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

    let c = create_client(args.local_address, core.handle(), logger.clone(), args.verbose);

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

    let con = c.borrow().connection_manager.get_connection(args.address)
        .unwrap();

    // Benchmark sending messages
    let count = args.amount;
    let mut time_reporter = slog_perf::TimeReporter::new_with_level(
        "Message benchmark",
        logger.clone(),
        slog::Level::Info,
    );
    time_reporter.start("Messages");
    let start = Instant::now();

    //cpuprofiler::PROFILER.lock().unwrap().start("./message-bench.profile").unwrap();
    core.run(stream::iter_ok(0..count).and_then(move |i| {
        let mut cmd = cmd.clone();
        cmd.push("msg", format!("Hello {}", i));
        let packet = Packet::new(header.clone(), Data::Command(cmd));
        let packets = client::ClientConnection::get_packets(Rc::downgrade(&con));
        packets.send(packet)
    }).for_each(|_| future::ok(()))).unwrap();
    //cpuprofiler::PROFILER.lock().unwrap().stop().unwrap();

    time_reporter.finish();
    let dur = start.elapsed();

    info!(logger,
        "{} messages in {}.{:03}s",
        count,
        dur.as_secs(),
        dur.subsec_nanos() / 1_000_000,
    );
    let dur = dur / count;
    info!(logger,
        "{}.{:03}ms per message",
        dur.subsec_nanos() / 1_000_000,
        dur.subsec_nanos() / 1_000,
    );

    // Wait some time
    let action = Timeout::new(Duration::from_secs(1), &core.handle()).unwrap();
    core.run(action).unwrap();

    // Disconnect
    if let Err(error) = core.run(disconnect(c.clone(), args.address)) {
        error!(logger, "Failed to disconnect"; "error" => ?error);
        return;
    }
    info!(logger, "Disconnected");
}
