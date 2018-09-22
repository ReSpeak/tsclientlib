//! Benchmark the number of reconnects we can make per second.

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
use std::time::Instant;

use futures::{future, Future};
use slog::Drain;
use structopt::StructOpt;
use structopt::clap::AppSettings;

mod utils;
use utils::*;

#[derive(StructOpt, Debug, Clone)]
#[structopt(raw(global_settings =
    "&[AppSettings::ColoredHelp, AppSettings::VersionlessSubcommands]"))]
struct Args {
    #[structopt(short = "a", long = "address",
                default_value = "127.0.0.1:9987",
                help = "The address of the server to connect to")]
    address: SocketAddr,
    #[structopt(short = "n", long = "amount", default_value = "5",
                help = "The amount of connections")]
    amount: u32,
    #[structopt(long = "local-address", default_value = "0.0.0.0:0",
                help = "The listening address of the client")]
    local_address: SocketAddr,
    #[structopt(short = "v", long = "verbose",
                help = "Display the content of all packets")]
    verbose: bool,
}

fn connect_once(args: Args, logger: slog::Logger) {
    tokio::run(future::lazy(move || {
        // The TS server does not accept the 3rd reconnect from the same port
        // so we create a new client for every connection.
        let c = create_client(args.local_address, logger.clone(),
            SimplePacketHandler, args.verbose);

        info!(logger, "Connecting");
        let logger2 = logger.clone();
        connect(logger.clone(), c.clone(), args.address)
            .map_err(|e| panic!("Failed to connect ({:?})", e))
            .and_then(move |con| {
                info!(logger, "Disconnecting");
                disconnect(con).map_err(|e| panic!("Failed to disconnect ({:?})", e))
            })
            .and_then(move |_| {
                info!(logger2, "Disconnected");
                // Quit client
                drop(c);
                Ok(())
            })
    }));
}

fn main() {
    tsproto::init().unwrap();

    // Parse command line options
    let args = Args::from_args();

    let logger = {
        let decorator = slog_term::TermDecorator::new().build();
        let drain = slog_term::FullFormat::new(decorator).build().fuse();
        let drain = slog_async::Async::new(drain).build().fuse();

        slog::Logger::root(drain, o!())
    };

    // Benchmark reconnecting
    let mut time_reporter = slog_perf::TimeReporter::new_with_level(
        "Connection benchmark",
        logger.clone(),
        slog::Level::Info,
    );
    time_reporter.start("Connections");
    let start = Instant::now();
    let mut success_count = 0;
    //cpuprofiler::PROFILER.lock().unwrap().start("./connect-bench.profile").unwrap();
    for _ in 0..args.amount {
        // Wait a bit
        std::thread::sleep(std::time::Duration::from_millis(15));

        connect_once(args.clone(), logger.clone());
        success_count += 1;
    }
    //cpuprofiler::PROFILER.lock().unwrap().stop().unwrap();
    time_reporter.finish();
    let dur = start.elapsed();

    info!(logger,
        "{} connects in {}.{:03}s",
        success_count,
        dur.as_secs(),
        dur.subsec_nanos() / 1_000_000
    );
    let dur = dur / success_count;
    info!(logger,
        "{}.{:03}s per connect",
        dur.as_secs(),
        dur.subsec_nanos() / 1_000_000
    );
}
