use std::cell::RefCell;
use std::net::SocketAddr;
use std::rc::Rc;
use std::time::Duration;

use {slog, slog_perf};
use futures::{future, Future, Sink, Stream};
use tokio_core::reactor::{Handle, Timeout};
use tsproto::*;
use tsproto::algorithms as algs;
use tsproto::connectionmanager::{ConnectionManager, Resender, ResenderEvent};
use tsproto::crypto::EccKeyPrivP256;
use tsproto::packets::*;

pub mod voice;

pub fn create_client(local_address: SocketAddr, handle: Handle, logger: slog::Logger, log: bool) -> Rc<RefCell<client::ClientData>> {
    // Get P-256 ECDH key
    let private_key = EccKeyPrivP256::from_ts(
        "MG0DAgeAAgEgAiAIXJBlj1hQbaH0Eq0DuLlCmH8bl+veTAO2+\
        k9EQjEYSgIgNnImcmKo7ls5mExb6skfK2Tw+u54aeDr0OP1ITsC/50CIA8M5nm\
        DBnmDM/gZ//4AAAAAAAAAAAAAAAAAAAAZRzOI").unwrap();

    let c = client::ClientData::new(
        local_address,
        private_key,
        handle,
        true,
        connectionmanager::SocketConnectionManager::new(),
        logger,
    ).unwrap();

    // Set the data reference
    {
        let c2 = c.clone();
        let mut c = c.borrow_mut();
        c.connection_manager.set_data_ref(Rc::downgrade(&c2));
    }

    // Packet encoding
    client::default_setup(&c, log);

    c
}

pub fn connect(
    logger: slog::Logger,
    handle: &Handle,
    client: Rc<RefCell<client::ClientData>>,
    server_addr: SocketAddr,
) -> Box<Future<Item = (), Error = Error>> {
    let connect_fut = client::connect(&client, server_addr);

    // Listen for packets so we can answer them
    let packets = handler_data::Data::get_packets(Rc::downgrade(&client));

    let logger2 = logger.clone();
    let logger3 = logger.clone();
    let listen = packets
        .for_each(|_| future::ok(()))
        .map(move |()| info!(logger2, "Listening finished"))
        .map_err(move |error| error!(logger3, "Listening error";
            "error" => ?error));
    handle.spawn(listen);

    let handle2 = handle.clone();
    Box::new(connect_fut.and_then(move |_| {
        // Wait some time
        // TODO Document in protocol paper
        Timeout::new(Duration::from_millis(5), &handle2).unwrap().map_err(|e| e.into())
    }).and_then(move |()| {
        let private_key = EccKeyPrivP256::from_ts(
            "MG0DAgeAAgEgAiAIXJBlj1hQbaH0Eq0DuLlCmH8bl+veTAO2+\
            k9EQjEYSgIgNnImcmKo7ls5mExb6skfK2Tw+u54aeDr0OP1ITsC/50CIA8M5nm\
            DBnmDM/gZ//4AAAAAAAAAAAAAAAAAAAAZRzOI").unwrap();

        // Compute hash cash
        let mut time_reporter = slog_perf::TimeReporter::new_with_level(
            "Compute public key hash cash level", logger.clone(),
            slog::Level::Info);
        time_reporter.start("Compute public key hash cash level");
        let private_key_as_pub = private_key.to_pub();
        let offset = algs::hash_cash(&private_key_as_pub, 8).unwrap();
        let omega = private_key_as_pub.to_ts().unwrap();
        time_reporter.finish();
        info!(logger, "Computed hash cash level";
            "level" => algs::get_hash_cash_level(&omega, offset),
            "offset" => offset);

        // Create clientinit packet
        let header = Header::new(PacketType::Command);
        let mut command = commands::Command::new("clientinit");
        command.push("client_nickname", "Bot");
        command.push("client_version", "3.1.8 [Build: 1516614607]");
        command.push("client_platform", "Linux");
        command.push("client_input_hardware", "1");
        command.push("client_output_hardware", "1");
        command.push("client_default_channel", "");
        command.push("client_default_channel_password", "");
        command.push("client_server_password", "");
        command.push("client_meta_data", "");
        command.push("client_version_sign", "LJ5q+KWT4KwBX7oR/9j9A12hBrq5ds5ony99f9kepNmqFskhT7gfB51bAJNgAMOzXVCeaItNmc10F2wUNktqCw==");
        command.push("client_key_offset", offset.to_string());
        command.push("client_nickname_phonetic", "");
        command.push("client_default_token", "");
        command.push("client_badges", "Overwolf=0");
        command.push("hwid", "923f136fb1e22ae6ce95e60255529c00,d13231b1bc33edfecfb9169cc7a63bcc");
        let p_data = packets::Data::Command(command);
        let clientinit_packet = Packet::new(header, p_data);

        let sink = handler_data::Data::get_packets(Rc::downgrade(&client));
        sink.send((server_addr, clientinit_packet)).and_then(move |_| {
            client::wait_until_connected(&client, server_addr)
        })
    }))
}

pub fn disconnect(
    client: Rc<RefCell<client::ClientData>>,
    server_addr: SocketAddr,
) -> Box<Future<Item = (), Error = Error>> {
    let header = Header::new(PacketType::Command);
    let mut command = commands::Command::new("clientdisconnect");

    // Reason: Disconnect
    command.push("reasonid", "8");
    command.push("reasonmsg", "Bye");
    let p_data = packets::Data::Command(command);
    let packet = Packet::new(header, p_data);

    let con = client.borrow().connection_manager
        .get_connection(server_addr).unwrap();
    con.borrow_mut().resender.handle_event(ResenderEvent::Disconnecting);
    let sink = handler_data::Data::get_packets(Rc::downgrade(&client));
    Box::new(sink
        .send((server_addr, packet))
        .and_then(move |_| {
            client::wait_for_state(&client, server_addr, |state| {
                if let client::ServerConnectionState::Disconnected = *state {
                    true
                } else {
                    false
                }
            })
        }),
    )
}
