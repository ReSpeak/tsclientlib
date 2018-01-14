extern crate failure;
extern crate structopt;
#[macro_use]
extern crate structopt_derive;
extern crate tokio_core;
extern crate tsclientlib;

use std::net::SocketAddr;
use std::time::Duration;

use structopt::StructOpt;
use structopt::clap::AppSettings;
use tokio_core::reactor::{Core, Timeout};

use tsclientlib::{ConnectOptions, ConnectionManager, DisconnectOptions,
    MoveReason};

#[derive(StructOpt, Debug)]
#[structopt(global_settings_raw =
    "&[AppSettings::ColoredHelp, AppSettings::VersionlessSubcommands]")]
struct Args {
    #[structopt(short = "a", long = "address",
                default_value = "127.0.0.1:9987",
                help = "The address of the server to connect to")]
    address: SocketAddr,
}

fn main() {
    real_main().unwrap();
}

fn real_main() -> Result<(), failure::Error> {
    // Parse command line options
    let args = Args::from_args();
    let mut core = Core::new()?;

    let mut cm = ConnectionManager::new(core.handle());
    let con_config = ConnectOptions::from_address(args.address);

    // Connect
    let con_id = core.run(cm.add_connection(con_config))?;

    {
        let con = cm.get_connection(con_id).unwrap();
        let server = con.get_server();
        println!("Server welcome message: {}", sanitize(&*server.get_welcome_message()));
    }

    // Wait some time
    let action = Timeout::new(Duration::from_secs(1), &core.handle())?;
    core.run(action)?;

    // Disconnect
    core.run(cm.remove_connection(con_id, DisconnectOptions::new()
        .reason(MoveReason::Clientdisconnect)
        .message("Is this the real world?")))?;

    Ok(())
}

/// Only retain a certain set of characters.
fn sanitize(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric() || [' ', '\t', '-', '_', '"', '\''].contains(&c)).collect()
}
