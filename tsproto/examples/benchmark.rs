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
extern crate tomcrypt;
extern crate tsproto;

use std::cell::RefCell;
use std::net::SocketAddr;
use std::rc::Rc;
use std::time::Instant;

use futures::{future, Future, Sink, Stream};
use slog::Drain;
use structopt::StructOpt;
use structopt::clap::AppSettings;
use tokio_core::reactor::Core;
use tsproto::*;
use tsproto::algorithms as algs;
use tsproto::packets::*;

#[derive(StructOpt, Debug)]
#[structopt(global_settings_raw = "&[AppSettings::ColoredHelp, AppSettings::VersionlessSubcommands]")]
struct Args {
    #[structopt(short = "a", long = "address",
                default_value = "127.0.0.1:9987",
                help = "The address of the server to connect to")]
    address: SocketAddr,
    #[structopt(long = "local-address", default_value = "0.0.0.0:0",
                help = "The listening address of the client")]
    local_address: SocketAddr,
}

fn connect(
    logger: slog::Logger,
    client: Rc<RefCell<client::ClientData>>,
    server_addr: SocketAddr,
) -> Box<Future<Item = (), Error = errors::Error>> {
    Box::new(client::connect(client.clone(), server_addr).and_then(move |()| {
        let mut private_key = tomcrypt::EccKey::import(
            &base64::decode("MG0DAgeAAgEgAiAIXJBlj1hQbaH0Eq0DuLlCmH8bl+veTAO2+\
                k9EQjEYSgIgNnImcmKo7ls5mExb6skfK2Tw+u54aeDr0OP1ITsC/50CIA8M5nm\
                DBnmDM/gZ//4AAAAAAAAAAAAAAAAAAAAZRzOI").unwrap()).unwrap();

        // Compute hash cash
        let mut time_reporter = slog_perf::TimeReporter::new_with_level(
            "Compute public key hash cash level", logger.clone(),
            slog::Level::Info);
        time_reporter.start("Compute public key hash cash level");
        let offset = algs::hash_cash(&mut private_key, 8).unwrap();
        let omega = base64::encode(&private_key.export_public().unwrap());
        time_reporter.finish();
        info!(logger, "Computed hash cash level";
            "level" => algs::get_hash_cash_level(&omega, offset),
            "offset" => offset);

        // Create clientinit packet
        let header = Header::new(PacketType::Command);
        let mut command = commands::Command::new("clientinit");
        command.push("client_nickname", "Bot");
        command.push("client_version", "3.1.6 [Build: 1502873983]");
        command.push("client_platform", "Linux");
        command.push("client_input_hardware", "0");
        command.push("client_output_hardware", "0");
        command.push("client_default_channel", "");
        command.push("client_default_channel_password", "");
        command.push("client_server_password", "");
        command.push("client_meta_data", "");
        command.push("client_version_sign", "o+l92HKfiUF+THx2rBsuNjj/S1QpxG1fd5o3Q7qtWxkviR3LI3JeWyc26eTmoQoMTgI3jjHV7dCwHsK1BVu6Aw==");
        command.push("client_key_offset", offset.to_string());
        command.push("client_nickname_phonetic", "");
        command.push("client_default_token", "");
        command.push("hwid", "123,456");
        let p_data = packets::Data::Command(command);
        let clientinit_packet = Packet::new(header, p_data);

        let sink = client::ClientData::get_packets(client.clone());
        sink.send((server_addr, clientinit_packet)).and_then(move |_| {
            client::wait_until_connected(client, server_addr)
        })
    }))
}

fn disconnect(
    _logger: slog::Logger,
    client: Rc<RefCell<client::ClientData>>,
    server_addr: SocketAddr,
) -> Box<Future<Item = (), Error = errors::Error>> {
    let header = Header::new(PacketType::Command);
    let mut command = commands::Command::new("clientdisconnect");
    // Never times out
    //let mut command = Command::new("clientinitiv");

    // Reason: Disconnect
    command.push("reasonid", "8");
    command.push("reasonmsg", "Bye");
    let p_data = packets::Data::Command(command);
    let packet = Packet::new(header, p_data);
    Box::new(client::ClientData::get_packets(client.clone())
        .send((server_addr, packet))
        .and_then(move |_| {
            client::wait_for_state(client, server_addr, |state| {
                if let client::ServerConnectionState::Disconnected = *state {
                    true
                } else {
                    false
                }
            })
        }),
    )
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

    // Create ECDH key
    //let prng = tomcrypt::sprng();
    //let mut private_key = tryf!(tomcrypt::EccKey::new(prng, 32));
    let private_key = tomcrypt::EccKey::import(
        &base64::decode("MG0DAgeAAgEgAiAIXJBlj1hQbaH0Eq0DuLlCmH8bl+veTAO2+\
            k9EQjEYSgIgNnImcmKo7ls5mExb6skfK2Tw+u54aeDr0OP1ITsC/50CIA8M5nm\
            DBnmDM/gZ//4AAAAAAAAAAAAAAAAAAAAZRzOI").unwrap()).unwrap();

    let c = client::ClientData::new(
        args.local_address,
        private_key,
        core.handle(),
        true,
        logger.clone(),
    ).unwrap();
    client::default_setup(c.clone());

    // Listen for packets
    let listen = client::ClientData::get_packets(c.clone())
        .for_each(|_| future::ok(()))
        .map(|()| println!("Listening finished"))
        .map_err(|error| println!("Listening error: {:?}", error));
    core.handle().spawn(listen);

    let mut header = Header::default();
    header.set_type(PacketType::Command);
    let mut cmd = commands::Command::new("sendtextmessage");
    cmd.push("targetmode", "3");
    cmd.push("msg", "Hello");

    let packet = Packet::new(header, Data::Command(cmd));

    // Benchmark reconnecting
    let count = 20;
    let mut time_reporter = slog_perf::TimeReporter::new_with_level(
        "Connection benchmark",
        logger.clone(),
        slog::Level::Info,
    );
    time_reporter.start("Connections");
    let start = Instant::now();
    for _ in 0..count {
        // The TS server does not accept the 3rd reconnect from the same port
        //let action = Timeout::new(Duration::from_millis(20), &core.handle()).unwrap();
        //core.run(action).unwrap();

        info!(logger, "Connecting");
        core.run(connect(logger.clone(), c.clone(), args.address))
            .unwrap();
        info!(logger, "Writing message");
        let sink = client::ClientData::get_packets(c.clone());
        core.run(sink.send((args.address, packet.clone()))).unwrap();
        info!(logger, "Disconnecting");
        core.run(disconnect(logger.clone(), c.clone(), args.address)).unwrap();
    }
    time_reporter.finish();
    let dur = start.elapsed();

    info!(logger,
        "{} connects in {}.{:03}s",
        count,
        dur.as_secs(),
        dur.subsec_nanos() / 1000000
    );
    let dur = dur / count;
    info!(logger,
        "{}.{:03}s per connect",
        dur.as_secs(),
        dur.subsec_nanos() / 1000000
    );
}
