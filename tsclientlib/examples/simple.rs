extern crate failure;
extern crate futures;
extern crate structopt;
extern crate tokio_core;
extern crate tsclientlib;

use std::time::Duration;

use futures::Future;
use structopt::StructOpt;
use structopt::clap::AppSettings;
use tokio_core::reactor::{Core, Timeout};

use tsclientlib::{ConnectOptions, ConnectionManager, DisconnectOptions,
    Reason};

#[derive(StructOpt, Debug)]
#[structopt(raw(global_settings =
    "&[AppSettings::ColoredHelp, AppSettings::VersionlessSubcommands]"))]
struct Args {
    #[structopt(short = "a", long = "address",
                default_value = "localhost",
                help = "The address of the server to connect to")]
    address: String,
    #[structopt(short = "v", long = "verbose",
                help = "Print the content of all packets")]
    verbose: bool,
}

fn main() {
    real_main().unwrap();
}

fn real_main() -> Result<(), failure::Error> {
    // Parse command line options
    let args = Args::from_args();
    let mut core = Core::new()?;

    let mut cm = ConnectionManager::new(core.handle());
    let con_config = ConnectOptions::new(args.address)
        .log_packets(args.verbose);

    // Optionally set the key of this client, otherwise a new key is generated.
    let con_config = con_config.private_key_ts(
        "MG0DAgeAAgEgAiAIXJBlj1hQbaH0Eq0DuLlCmH8bl+veTAO2+\
        k9EQjEYSgIgNnImcmKo7ls5mExb6skfK2Tw+u54aeDr0OP1ITs\
        C/50CIA8M5nmDBnmDM/gZ//4AAAAAAAAAAAAAAAAAAAAZRzOI").unwrap();

    // Connect
    let con_id = core.run(cm.add_connection(con_config))?;

    {
        let con = cm.get_connection(con_id).unwrap();
        let server = con.get_server();
        println!("Server welcome message: {}", sanitize(&*server.get_welcome_message()));
    }

    // Wait some time
    let action = Timeout::new(Duration::from_secs(1), &core.handle())?;
    core.run(action.select2(cm.run())).unwrap();

    // Disconnect
    core.run(cm.remove_connection(con_id, DisconnectOptions::new()
        .reason(Reason::Clientdisconnect)
        .message("Is this the real world?")))?;

    Ok(())
}

/// Only retain a certain set of characters.
fn sanitize(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()
        || [' ', '\t', '-', '_', '"', '\'', '/', '(', ')', '[', ']', '{', '}']
        .contains(c)).collect()
}
