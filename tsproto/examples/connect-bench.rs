//! Benchmark the number of reconnects we can make per second.

extern crate base64;
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
use std::time::Instant;

use slog::Drain;
use structopt::StructOpt;
use structopt::clap::AppSettings;
use tokio_core::reactor::Core;

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
    #[structopt(short = "n", long = "amount", default_value = "20",
                help = "The amount of connections")]
    amount: u32,
    #[structopt(long = "local-address", default_value = "0.0.0.0:0",
                help = "The listening address of the client")]
    local_address: SocketAddr,
}

fn connect_once(core: &mut Core, args: &Args, logger: &slog::Logger)
    -> Result<(), tsproto::Error> {
    // Wait a bit
    std::thread::sleep(std::time::Duration::from_millis(15));

    let c = create_client(args.local_address, core.handle(), logger.clone(), false);

    let handle = core.handle();
    // The TS server does not accept the 3rd reconnect from the same port
    let action = tokio_core::reactor::Timeout::new(
        std::time::Duration::from_millis(2), &handle).unwrap();
    core.run(action).unwrap();

    info!(logger, "Connecting");
    core.run(connect(logger.clone(), &handle, c.clone(), args.address))?;

    info!(logger, "Disconnecting");
    core.run(disconnect(c.clone(), args.address)).unwrap();
    Ok(())
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

    // Benchmark reconnecting
    let mut time_reporter = slog_perf::TimeReporter::new_with_level(
        "Connection benchmark",
        logger.clone(),
        slog::Level::Info,
    );
    time_reporter.start("Connections");
    let start = Instant::now();
    let mut success_count = 0;
    for _ in 0..args.amount {
        if let Err(error) = connect_once(&mut core, &args, &logger) {
            error!(logger, "Failed to connect"; "error" => ?error);
            break;
        }
        success_count += 1;
    }
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
