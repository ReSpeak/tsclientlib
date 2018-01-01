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
use std::time::Duration;

use futures::{future, Future, Sink, Stream};
use slog::Drain;
use structopt::StructOpt;
use structopt::clap::AppSettings;
use tokio_core::reactor::{Core, Handle, Timeout};
use tsproto::*;
use tsproto::algorithms as algs;
use tsproto::connectionmanager::ConnectionManager;
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
    handle: &Handle,
    client: Rc<RefCell<client::ClientData>>,
    server_addr: SocketAddr,
) -> Box<Future<Item = (), Error = errors::Error>> {
    let connect_fut = client::connect(client.clone(), server_addr);

    // Listen for packets so we can answer them
    let con = client.borrow().connection_manager.get_connection(server_addr)
        .unwrap();
    let packets = client::ClientConnection::get_packets(con.clone());

    let logger2 = logger.clone();
    let logger3 = logger.clone();
    let listen = packets
        .for_each(|_| future::ok(()))
        .map(move |()| info!(logger2, "Listening finished"))
        .map_err(move |error| error!(logger3, "Listening error";
            "error" => ?error));
    handle.spawn(listen);

    Box::new(connect_fut.and_then(move |()| {
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
        command.push("client_input_hardware", "1");
        command.push("client_output_hardware", "1");
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

        let con = client.borrow().connection_manager
            .get_connection(server_addr).unwrap();
        let sink = client::ClientConnection::get_packets(con);
        sink.send(clientinit_packet).and_then(move |_| {
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

    let con = client.borrow().connection_manager
        .get_connection(server_addr).unwrap();
    let sink = client::ClientConnection::get_packets(con);
    Box::new(sink
        .send(packet)
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
        connectionmanager::SocketConnectionManager::new(),
        logger.clone(),
    ).unwrap();

    // Set the data reference
    {
        let c2 = c.clone();
        let mut c = c.borrow_mut();
        c.connection_manager.set_data_ref(c2);
    }

    // Packet encoding
    client::default_setup(c.clone(), false);

    // Connect
    let handle = core.handle();
    if let Err(error) = core.run(connect(logger.clone(), &handle, c.clone(),
        args.address)) {
        error!(logger, "Failed to connect"; "error" => ?error);
        return;
    }
    info!(logger, "Connected");

    // Build packet
    #[cfg(foo)]
    {
    let mut sink = client::ClientData::get_packets(c.clone());
    let mut header = Header::default();
    header.set_type(PacketType::Command);
    let mut cmd = commands::Command::new("sendtextmessage");
    cmd.push("targetmode", "3");
    cmd.push("msg", "");

    let packet = Packet::new(header, Data::Command(cmd));
    for i in 0..(3 * 65540) {
        sink = core.run(sink.send((args.address, packet.clone()))).unwrap();
        if i & 0x3fff == 0 {
            info!(logger, "Sending packet {:#x}", i);
        }
        if i & 0xf == 0 {
            //let action = Timeout::new(Duration::from_millis(1), &core.handle()).unwrap();
            //core.run(action).unwrap();
        }
    }
    }

    // Build voice packet
    {
    let mut header = Header::default();
    header.set_type(PacketType::Voice);
    let voice = Data::Voice {
        id: 0,
        codec_type: 0,
        voice_data: vec![0x3, 0x4, 0x78, 0x82, 0xbe, 0xe0, 0x88, 0x13, 0x79, 0xd3, 0xbc, 0xf6, 0xd2, 0x6c, 0xfd, 0x80, 0x36, 0x5e, 0x8a, 0xaa, 0xf, 0x2a, 0xe0, 0xa7, 0xbd, 0x68, 0x3f, 0xd7, 0x44, 0xe2, 0xcb, 0x8e, 0x48, 0x68, 0x75, 0x7c, 0x6b, 0x47, 0x42, 0x68, 0x61, 0xc9, 0xf9, 0x44, 0xa9, 0xfc, 0xc4, 0xf3, 0x8c, 0xfe, 0x26, 0x68, 0x2e, 0x66, 0x12, 0xac, 0x1c, 0x61, 0xa8, 0x1b, 0x6a, 0xfc, 0x55, 0xd8, 0xa1, 0x7, 0x25, 0x87, 0xc4, 0x89, 0x8c, 0x5c],
    };

    let con = c.borrow().connection_manager.get_connection(args.address)
        .unwrap();
    let mut packets = client::ClientConnection::get_packets(con);
    let mut packet = Packet::new(header, voice);
    for i in 0..(3 * 65540) {
        if let Data::Voice { ref mut id, .. } = packet.data {
            *id = i as u16;
        }
        packets = core.run(packets.send(packet.clone())).unwrap();
        if i & 0x3fff == 0 {
            info!(logger, "Sending packet {:#x}", i);
        }
        let action = Timeout::new(Duration::from_millis(1), &core.handle()).unwrap();
        core.run(action).unwrap();
    }
    }

    // Wait some time
    let action = Timeout::new(Duration::from_secs(3), &core.handle()).unwrap();
    core.run(action).unwrap();

    // Disconnect
    core.run(disconnect(logger.clone(), c.clone(), args.address)).unwrap();
    info!(logger, "Disconnected");
}
