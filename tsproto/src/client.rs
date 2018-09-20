use std::cell::RefCell;
use std::net::SocketAddr;
use std::rc::{Rc, Weak};

use {base64, tokio};
use chrono::Utc;
use futures::{self, future, Future, Sink, Stream};
use futures::sync::mpsc;
#[cfg(feature = "rust-gmp")]
use gmp::mpz::Mpz;
#[cfg(not(feature = "rust-gmp"))]
use num::One;
use num::ToPrimitive;
#[cfg(not(feature = "rust-gmp"))]
use num::bigint::BigUint;
use rand::{self, Rng};
use slog::Logger;

use {packets, BoxFuture, Error, Result};
use algorithms as algs;
use commands::Command;
use connection::*;
use connectionmanager::{ConnectionManager, Resender, ResenderEvent,
    SocketConnectionManager};
use crypto::{EccKeyPrivP256, EccKeyPubP256, EccKeyPrivEd25519};
use handler_data::{ConnectionValue, Data, DataM, PacketHandler};
use license::Licenses;
use packets::*;
use utils::MultiSink;

/// The data of our client.
pub type CM = SocketConnectionManager<DefaultPacketHandler, ServerConnectionData>;
pub type ClientData = Data<CM>;
pub type ClientDataM = DataM<CM>;
/// Connections from a client to a server.
pub type ClientConnection = Connection;
pub type ClientConVal = ConnectionValue<ServerConnectionData>;

pub struct ServerConnectionData {
    /// Every function in this list is called when the state of the connection
    /// changes.
    ///
    /// Return `false` to remain in the list of listeners.
    /// If `true` is returned, this listener will be removed.
    pub state_change_listener: Vec<Box<FnMut(&ServerConnectionState) -> bool + Send>>,
    pub state: ServerConnectionState,
}

#[derive(Debug)]
pub enum ServerConnectionState {
    /// After `Init0` was sent.
    Init0 { version: u32, random0: [u8; 4] },
    /// After `Init2` was sent.
    Init2 { version: u32 },
    /// After `Init4` was sent.
    ClientInitIv { alpha: [u8; 10] },
    /// The initial handshake is done and the next packet has to be
    /// `clientinit`.
    Connecting,
    /// Fully connected, the client id is known.
    Connected,
    /// The connection is finished, no more packets can be sent or received.
    Disconnected,
}

fn create_init_header() -> Header {
    let mut mac = [0; 8];
    mac.copy_from_slice(b"TS3INIT1");
    let mut header = Header {
        mac,
        p_id: 0x65,
        c_id: Some(0),
        p_type: 0,
    };
    header.set_type(PacketType::Init);
    header.set_unencrypted(true);
    header
}

/// Configures the default setup chain, including logging and decoding
/// of packets.
pub fn default_setup(data: &Rc<RefCell<Data<CM>>>, log: bool) {
    if log {
        // Logging
        let a = {
            let data = data.borrow();
            (data.logger.clone(), data.is_client)
        };
        // TODO Packet logging
    }

    // TODO Default packet handler
}

/// Wait until a client reaches a certain state.
///
/// `is_state` should return `true`, if the state is reached and `false` if this
/// function should continue waiting.
pub fn wait_for_state<F: Fn(&ServerConnectionState) -> bool + Send + 'static>(
    connection: &ClientConVal,
    f: F,
) -> impl Future<Item=(), Error=Error> {
    let (send, recv) = mpsc::channel(0);
    connection.mutex.with(move |mut c| -> Result<()> {
        c.0.state_change_listener.push(Box::new(move |s| {
            let mut send = send.clone();
            // Check if it is the right state
            if f(s) {
                if send.try_send(()).is_err() {
                    tokio::spawn(send.send(()).map(|_| ()).map_err(|e| {
                        println!("Failed to send while waiting for state ({:?})", e);
                    }));
                }
                true
            } else {
                false
            }
        }));
        Ok(())
    }).unwrap().from_err().and_then(move |_| recv.into_future().map(|_| ())
        .map_err(|e| format_err!("Failed to receive while waiting for state \
            ({:?})", e).into()))
}

pub fn wait_until_connected(
    connection: &ClientConVal,
) -> impl Future<Item=(), Error=Error> {
    wait_for_state(connection, |state| {
        if let ServerConnectionState::Connected = *state {
            true
        } else {
            false
        }
    })
}

/// Connect to a server.
///
/// This function returns, when the client reached the
/// [`ServerConnectionState::Connecting`] state. Then the client should send the
/// `clientinit` packet and call [`wait_until_connected`].
///
/// [`ServerConnectionState::Connecting`]:
/// [`wait_until_connected`]:
pub fn connect(
    data: &mut ClientData,
    server_addr: SocketAddr,
) -> impl Future<Item=(), Error=Error> {
    // Send the first init packet
    // Get the current timestamp
    let now = Utc::now();
    let timestamp = now.timestamp() as u32;
    let mut rng = rand::thread_rng();

    // Random bytes
    let random0 = rng.gen::<[u8; 4]>();
    let packet_data = C2SInit::Init0 {
        version: timestamp,
        timestamp,
        random0,
    };
    let cheader = create_init_header();

    let state = ServerConnectionData {
        state_change_listener: Vec::new(),
        state: ServerConnectionState::Init0 {
            version: timestamp,
            random0,
        },
    };
    // Add the connection to the connection list
    let con = data.create_connection(server_addr);
    let key = data.add_connection(state, con);
    let con = data.get_connection(&key).unwrap();

    let packet = Packet::new(cheader, packets::Data::C2SInit(packet_data));
    let con2 = con.clone();
    con.as_packet_sink().send(packet).and_then(move |_| {
        wait_for_state(&con2, |state| {
            if let ServerConnectionState::Connecting = *state {
                true
            } else {
                false
            }
        })
    })
}

#[derive(Clone, Debug)]
pub struct DefaultPacketHandler;

impl PacketHandler<ServerConnectionData> for DefaultPacketHandler {
    fn new_connection<S1, S2>(
        &mut self,
        con_val: ClientConVal,
        con: &Connection,
        command_stream: S1,
        audio_stream: S2,
    ) where
        S1: Stream<Item=Packet, Error=Error>,
        S2: Stream<Item=Packet, Error=Error> {
    }
}

impl DefaultPacketHandler {
    fn handle_init_packet(
        con_value: ConnectionValue<CM>,
        (state, con): &mut (ServerConnectionData, Connection),
        packet: &Packet,
        ignore_packet: &mut bool,
        is_end: &mut bool,
        private_key: &EccKeyPrivP256,
        logger: &Logger,
    ) -> Result<Option<(ServerConnectionState, Option<Packet>)>> {
        let res = match state.state {
            ServerConnectionState::Init0 { version, ref random0 } => {
                // Handle an Init1
                if let Packet { data: packets::Data::S2CInit(
                    S2CInit::Init1 { ref random1, ref random0_r }), .. } = *packet {
                    // Check the response
                    if random0.as_ref().iter().rev().eq(random0_r.as_ref()) {
                        // The packet is correct.
                        // Send next init packet
                        let cheader = create_init_header();
                        let data = C2SInit::Init2 {
                            version,
                            random1: *random1,
                            random0_r: *random0_r,
                        };

                        let state = ServerConnectionState::Init2 {
                            version,
                        };

                        Some((state, Some(Packet::new(cheader,
                            packets::Data::C2SInit(data)))))
                    } else {
                        return Err(format_err!("Init: Got wrong data in the Init1 response packet").into());
                    }
                } else {
                    None
                }
            }
            ServerConnectionState::Init2 { version } => {
                // Handle an Init3
                if let Packet { data: packets::Data::S2CInit(
                    S2CInit::Init3 { ref x, ref n, level, ref random2 }), .. } = *packet {
                    // Solve RSA puzzle: y = x ^ (2 ^ level) % n
                    // Use Montgomery Reduction
                    if level > 10_000_000 {
                        // Reject too high exponents
                        None
                    } else {
                        // Create clientinitiv
                        let mut rng = rand::thread_rng();
                        let alpha = rng.gen::<[u8; 10]>();
                        // omega is an ASN.1-DER encoded public key from
                        // the ECDH parameters.

                        let alpha_s = base64::encode(&alpha);
                        let priv_pub_key = private_key.to_pub();
                        let ip = con.address.ip();
                        let logger = logger.clone();
                        let random2 = *random2;
                        let x = *x;
                        let n = *n;
                        // Spawn this as another future
                        let logger2 = logger.clone();
                        let fut = future::lazy(move || {
                            let mut time_reporter = ::slog_perf::TimeReporter::new_with_level(
                                "Solve RSA puzzle", logger.clone(),
                                ::slog::Level::Info);
                            time_reporter.start("");

                            // TODO Blocking operation with timeout
                            // Use gmp for faster computations if it is
                            // available.
                            #[cfg(feature = "rust-gmp")]
                            let y = {
                                let n = (&n as &[u8]).into();
                                let x: Mpz = (&x as &[u8]).into();
                                let mut e = Mpz::new();
                                e.setbit(level as usize);
                                let y = x.powm(&e, &n);
                                time_reporter.finish();
                                info!(logger, "Solve RSA puzzle";
                                      "level" => level);
                                let ys = y.to_str_radix(10);
                                let yi = ys.parse().unwrap();
                                algs::biguint_to_array(&yi)
                            };

                            #[cfg(not(feature = "rust-gmp"))]
                            let y = {
                                let xi = BigUint::from_bytes_be(&x);
                                let ni = BigUint::from_bytes_be(&n);
                                let mut e = BigUint::one();
                                e <<= level as usize;
                                let yi = xi.modpow(&e, &ni);
                                time_reporter.finish();
                                info!(logger, "Solve RSA puzzle";
                                      "level" => level, "x" => %xi, "n" => %ni,
                                      "y" => %yi);
                                algs::biguint_to_array(&yi)
                            };

                            // Create the command string
                            // omega is an ASN.1-DER encoded public key from
                            // the ECDH parameters.
                            let omega_s = priv_pub_key.to_ts().unwrap();
                            let mut command = Command::new("clientinitiv");
                            command.push("alpha", alpha_s);
                            command.push("omega", omega_s);
                            command.push("ot", "1");
                            // Set ip always except if it is a local address
                            if ::utils::is_global_ip(&ip) {
                                command.push("ip", ip.to_string());
                            } else {
                                command.push("ip", "");
                            }

                            let cheader = create_init_header();
                            let data = C2SInit::Init4 {
                                version,
                                x,
                                n,
                                level,
                                random2,
                                y,
                                command: command.clone(),
                            };

                            let packet = Packet::new(cheader, packets::Data::C2SInit(data));
                            con_value.as_packet_sink().send(packet)
                        }).map(|_| ()).map_err(move |error|
                            error!(logger2, "Cannot send packet";
                                "error" => ?error));
                        tokio::spawn(fut);

                        let state = ServerConnectionState::ClientInitIv {
                            alpha,
                        };

                        *ignore_packet = true;
                        Some((state, None))
                    }
                } else {
                    None
                }
            }
            ServerConnectionState::Disconnected => {
                warn!(logger, "Got packet from server after disconnecting");
                *is_end = true;
                None
            }
            _ => None,
        };
        Ok(res)
    }
}

/*struct DefaultPacketHandlerStream;

impl DefaultPacketHandlerStream {
    pub fn new<
        InnerStream: Stream<Item = (<CM as ConnectionManager>::Key, Packet), Error = Error> + 'static,
        InnerSink: Sink<SinkItem = (<CM as ConnectionManager>::Key, Packet), SinkError = Error> + 'static,
    >(
        data: &Rc<RefCell<Data<CM>>>,
        inner_stream: InnerStream,
        inner_sink: InnerSink,
    ) -> (Box<Stream<Item = (<CM as ConnectionManager>::Key, Packet), Error = Error>>,
        MultiSink<InnerSink>) {
        let sink = MultiSink::new(inner_sink);
        let sink2 = sink.clone();
        let data = Rc::downgrade(data);
        let inner_stream = Box::new(inner_stream.and_then(move |(key, packet)|
            -> BoxFuture<_, _> {
            // true, if the packet should not be handled further.
            let mut ignore_packet = true;
            // If the connection should be removed
            let mut is_end = false;
            // Check if we have a connection for this server
            let packet_res = {
                let data = data.upgrade().unwrap();
                let mut data = data.borrow_mut();
                let data = &mut *data;
                if let Some(con) = data.connection_manager
                    .get_connection(key.clone()) {
                    let mut con = con.borrow_mut();
                    let logger = con.logger.clone();
                    let state = data.connection_manager
                        .get_mut_data(key.clone()).unwrap();
                    let handle_res = match Self::handle_packet::<_>(state, &packet,
                        &mut ignore_packet, &mut is_end, &data.private_key,
                        &mut con, key.clone(), &logger,
                        sink.clone()) {
                        Ok(res) => res,
                        Err(error) => {
                            error!(logger, "Error when handling packet";
                                "error" => ?error);
                            None
                        }
                    };
                    if let Some((s, packet)) = handle_res {
                        state.state = s;
                        Some(packet)
                    } else {
                        None
                    }
                } else {
                    None
                }
            };

            if is_end {
                Data::remove_connection(&data.upgrade().unwrap(), &key);
            }

            if let Some(p) = packet_res {
                // First send the packet, then notify the listeners, this
                // ensures that the clientek packet is sent before the
                // clientinit.
                let res_fut: BoxFuture<(), _> = if let Some(p) = p {
                    // Send the packet
                    Box::new(sink.clone().send((key.clone(), p)).map(|_| ()))
                } else {
                    Box::new(future::ok(()))
                };

                Box::new(res_fut.and_then(move |_| {
                    // TODO Lock connection
                    Ok(panic!())
                }).and_then(move |con| {
                    let state = &mut con.0;
                    // Notify state changed listeners
                    let mut i = 0;
                    while i < state.state_change_listener.len() {
                        if state.state_change_listener[i](&state.state) {
                            state.state_change_listener.remove(i);
                        } else {
                            i += 1;
                        }
                    }
                }).and_then(move |_|
                    if ignore_packet {
                        future::ok(None)
                    } else {
                        future::ok(Some((key, packet)))
                    }))
            } else if ignore_packet || is_end {
                Box::new(future::ok(None))
            } else {
                Box::new(future::ok(Some((key, packet))))
            }
        })
        .filter_map(|p| p));
        (inner_stream, sink2)
    }

    fn handle_packet<
        InnerSink: Sink<SinkItem = (<CM as ConnectionManager>::Key, Packet), SinkError = Error> + 'static,
    >(state: &mut ServerConnectionData, packet: &Packet,
        ignore_packet: &mut bool, is_end: &mut bool,
        private_key: &EccKeyPrivP256, con: &mut Connection,
        con_key: <CM as ConnectionManager>::Key, logger: &Logger,
        sink: MultiSink<InnerSink>)
        -> Result<Option<(ServerConnectionState, Option<Packet>)>> {
        let res = match state.state {
            ServerConnectionState::Init0 { version, ref random0 } => {
                // Handle an Init1
                if let Packet { data: packets::Data::S2CInit(
                    S2CInit::Init1 { ref random1, ref random0_r }), .. } = *packet {
                    // Check the response
                    if random0.as_ref().iter().rev().eq(random0_r.as_ref()) {
                        // The packet is correct.
                        // Send next init packet
                        let cheader = create_init_header();
                        let data = C2SInit::Init2 {
                            version,
                            random1: *random1,
                            random0_r: *random0_r,
                        };

                        let state = ServerConnectionState::Init2 {
                            version,
                        };

                        Some((state, Some(Packet::new(cheader,
                            packets::Data::C2SInit(data)))))
                    } else {
                        return Err(format_err!("Init: Got wrong data in the Init1 response packet").into());
                    }
                } else {
                    None
                }
            }
            ServerConnectionState::Init2 { version } => {
                // Handle an Init3
                if let Packet { data: packets::Data::S2CInit(
                    S2CInit::Init3 { ref x, ref n, level, ref random2 }), .. } = *packet {
                    // Solve RSA puzzle: y = x ^ (2 ^ level) % n
                    // Use Montgomery Reduction
                    if level > 10_000_000 {
                        // Reject too high exponents
                        None
                    } else {
                        // Create clientinitiv
                        let mut rng = rand::thread_rng();
                        let alpha = rng.gen::<[u8; 10]>();
                        // omega is an ASN.1-DER encoded public key from
                        // the ECDH parameters.

                        let alpha_s = base64::encode(&alpha);
                        let priv_pub_key = private_key.to_pub();
                        let ip = con.address.ip();
                        let logger = logger.clone();
                        let random2 = *random2;
                        let x = *x;
                        let n = *n;
                        // Spawn this as another future
                        let logger2 = logger.clone();
                        let fut = future::lazy(move || {
                            let mut time_reporter = ::slog_perf::TimeReporter::new_with_level(
                                "Solve RSA puzzle", logger.clone(),
                                ::slog::Level::Info);
                            time_reporter.start("");

                            // Use gmp for faster computations if it is
                            // available.
                            #[cfg(feature = "rust-gmp")]
                            let y = {
                                let n = (&n as &[u8]).into();
                                let x: Mpz = (&x as &[u8]).into();
                                let mut e = Mpz::new();
                                e.setbit(level as usize);
                                let y = x.powm(&e, &n);
                                time_reporter.finish();
                                info!(logger, "Solve RSA puzzle";
                                      "level" => level);
                                let ys = y.to_str_radix(10);
                                let yi = ys.parse().unwrap();
                                algs::biguint_to_array(&yi)
                            };

                            #[cfg(not(feature = "rust-gmp"))]
                            let y = {
                                let xi = BigUint::from_bytes_be(&x);
                                let ni = BigUint::from_bytes_be(&n);
                                let mut e = BigUint::one();
                                e <<= level as usize;
                                let yi = xi.modpow(&e, &ni);
                                time_reporter.finish();
                                info!(logger, "Solve RSA puzzle";
                                      "level" => level, "x" => %xi, "n" => %ni,
                                      "y" => %yi);
                                algs::biguint_to_array(&yi)
                            };

                            // Create the command string
                            // omega is an ASN.1-DER encoded public key from
                            // the ECDH parameters.
                            let omega_s = priv_pub_key.to_ts().unwrap();
                            let mut command = Command::new("clientinitiv");
                            command.push("alpha", alpha_s);
                            command.push("omega", omega_s);
                            command.push("ot", "1");
                            // Set ip always except if it is a local address
                            if ::utils::is_global_ip(&ip) {
                                command.push("ip", ip.to_string());
                            } else {
                                command.push("ip", "");
                            }

                            let cheader = create_init_header();
                            let data = C2SInit::Init4 {
                                version,
                                x,
                                n,
                                level,
                                random2,
                                y,
                                command: command.clone(),
                            };

                            let packet = Packet::new(cheader, packets::Data::C2SInit(data));
                            sink.send((con_key, packet))
                        }).map(|_| ()).map_err(move |error|
                            error!(logger2, "Cannot send packet";
                                "error" => ?error));
                        tokio::spawn(fut);

                        let state = ServerConnectionState::ClientInitIv {
                            alpha,
                        };

                        *ignore_packet = true;
                        Some((state, None))
                    }
                } else {
                    None
                }
            }
            ServerConnectionState::ClientInitIv { ref alpha } => {
                let res = (|con_params: &mut Option<ConnectedParams>| -> Result<_> {
                    if let Packet { data: packets::Data::Command(ref command), .. } = *packet {
                        let cmd = command.get_commands().remove(0);
                        if cmd.command == "initivexpand"
                            && cmd.has_arg("alpha")
                            && cmd.has_arg("beta")
                            && cmd.has_arg("omega")
                            && base64::decode(cmd.args["alpha"])
                            .map(|a| a == alpha).unwrap_or(false) {

                            let beta_vec = base64::decode(cmd.args["beta"])?;
                            if beta_vec.len() != 10 {
                                return Err(format_err!(
                                    "Incorrect beta length"))?;
                            }

                            let mut beta = [0; 10];
                            beta.copy_from_slice(&beta_vec);
                            let mut server_key = EccKeyPubP256::
                                from_ts(cmd.args["omega"])?;

                            let (iv, mac) = algs::compute_iv_mac(alpha, &beta,
                                private_key.clone(), server_key.clone())?;
                            let mut params = ConnectedParams::new(
                                server_key, SharedIv::ProtocolOrig(iv), mac);
                            // We already sent a command packet.
                            params.outgoing_p_ids[PacketType::Command.to_usize().unwrap()]
                                .1 = 1;
                            // We received a command packet.
                            params.incoming_p_ids[PacketType::Command.to_usize().unwrap()]
                                .1 = 1;
                            // And we sent an ack.
                            params.incoming_p_ids[PacketType::Ack.to_usize().unwrap()]
                                .1 = 1;
                            *con_params = Some(params);
                            Ok(None)
                        } else if cmd.command == "initivexpand2"
                            && cmd.has_arg("l")
                            && cmd.has_arg("beta")
                            && cmd.has_arg("omega")
                            && cmd.has_arg("ot")
                            && cmd.args["ot"] == "1"
                            && cmd.has_arg("time")
                            && cmd.has_arg("beta") {

                            let mut server_key = EccKeyPubP256::
                                from_ts(cmd.args["omega"])?;
                            let l = base64::decode(cmd.args["l"])?;
                            let proof = base64::decode(cmd.args["proof"])?;
                            // Check signature of l (proof)
                            server_key.clone().verify(&l, &proof)?;

                            let beta_vec = base64::decode(cmd.args["beta"])?;
                            if beta_vec.len() != 54 {
                                return Err(format_err!(
                                    "Incorrect beta length").into());
                            }

                            let mut beta = [0; 54];
                            beta.copy_from_slice(&beta_vec);

                            // Parse license argument
                            let licenses = Licenses::parse(&l)?;
                            // Ephemeral key of server
                            let server_ek = licenses.derive_public_key()?;

                            // Create own ephemeral key
                            let ek = EccKeyPrivEd25519::create()?;

                            let (iv, mac) = algs::compute_iv_mac31(alpha,
                                &beta, &ek, &server_ek)?;
                            let mut params = ConnectedParams::new(
                                server_key, SharedIv::Protocol31(iv), mac);
                            // We already sent a command packet.
                            params.outgoing_p_ids[PacketType::Command.to_usize().unwrap()]
                                .1 = 1;
                            // We received a command packet.
                            params.incoming_p_ids[PacketType::Command.to_usize().unwrap()]
                                .1 = 1;
                            // And we sent an ack.
                            params.incoming_p_ids[PacketType::Ack.to_usize().unwrap()]
                                .1 = 1;
                            *con_params = Some(params);

                            // Send clientek
                            let mut command = Command::new("clientek");
                            let ek_pub = ek.to_pub();
                            let ek_s = base64::encode(ek_pub.0.as_bytes());

                            // Proof: ECDSA signature of ek || beta
                            let mut all = Vec::with_capacity(32 + 54);
                            all.extend_from_slice(ek_pub.0.as_bytes());
                            all.extend_from_slice(&beta);
                            let proof = private_key.clone().sign(&all)?;
                            let proof_s = base64::encode(&proof);

                            command.push("ek", ek_s);
                            command.push("proof", proof_s);

                            let mut cheader = Header::new(PacketType::Command);

                            Ok(Some(Packet::new(cheader,
                                packets::Data::Command(command))))
                        } else {
                            Err(format_err!("initivexpand command has wrong arguments").into())
                        }
                    } else {
                        Ok(None)
                    }})(&mut con.params);

                match res {
                    Ok(p) => {
                        Some((ServerConnectionState::Connecting, p))
                    }
                    Err(error) => {
                        error!(logger, "Handle udp init packet"; "error" => %error);
                        None
                    }
                }
            }
            ServerConnectionState::Connecting => {
                let mut res = None;
                if let Packet { data: packets::Data::Command(ref cmd), .. } = *packet {
                    let cmd = cmd.get_commands().remove(0);
                    if cmd.command == "initserver" && cmd.has_arg("aclid") {
                        // Handle an initserver
                        if let Some(ref mut params) = con.params {
                            if let Ok(c_id) = cmd.args["aclid"].parse() {
                                params.c_id = c_id;
                            }
                        }
                        // initserver is the ack for clientinit
                        // Remove from send queue
                        con.resender.ack_packet(PacketType::Command, 1);
                        // Notify the resender that we are connected
                        con.resender.handle_event(ResenderEvent::Connected);
                        res = Some((ServerConnectionState::Connected, None));
                        *ignore_packet = false;
                    }
                }
                res
            }
            ServerConnectionState::Connected => {
                *ignore_packet = false;
                let mut res = None;
                if let Packet { data: packets::Data::Command(ref cmd), .. } = *packet {
                    let cmd = cmd.get_commands().remove(0);
                    if cmd.command == "notifyclientleftview" && cmd.has_arg("clid") {
                        // Handle a disconnect
                        if let Some(ref mut params) = con.params {
                            if cmd.args["clid"].parse() == Ok(params.c_id) {
                                *is_end = true;
                                res = Some((ServerConnectionState::Disconnected, None));
                            }
                        }
                    } else if cmd.command == "notifyplugincmd"
                        && cmd.has_arg("name") && cmd.has_arg("data")
                        && cmd.args["name"] == "cliententerview" {
                        if cmd.args["data"] == "version" {
                            let mut command = Command::new("plugincmd");
                            command.push("name", "cliententerview");
                            command.push("data", format!("{},{}-{}",
                                con.params.as_ref().unwrap().c_id,
                                env!("CARGO_PKG_NAME"),
                                env!("CARGO_PKG_VERSION"),
                            ));
                            command.push("targetmode", "1");

                            let header = Header::new(PacketType::Command);

                            let p = Some(Packet::new(header,
                                packets::Data::Command(command)));
                            res = Some((ServerConnectionState::Connected, p));
                        }
                    }
                }
                res
            }
            ServerConnectionState::Disconnected => {
                warn!(logger, "Got packet from server after disconnecting");
                *is_end = true;
                None
            }
        };
        Ok(res)
    }
}

pub struct DefaultPacketHandlerSink<
    InnerSink: Sink<SinkItem = (<CM as ConnectionManager>::Key, Packet), SinkError = Error> + 'static,
> {
    data: Weak<RefCell<Data<CM>>>,
    inner_sink: MultiSink<InnerSink>,
}

impl<
    InnerSink: Sink<SinkItem = (<CM as ConnectionManager>::Key, Packet), SinkError = Error> + 'static,
> DefaultPacketHandlerSink<InnerSink> {
    pub fn new(
        data: Weak<RefCell<Data<CM>>>,
        inner_sink: MultiSink<InnerSink>,
    ) -> Self {
        Self { data, inner_sink }
    }
}

impl<
    InnerSink: Sink<SinkItem = (<CM as ConnectionManager>::Key, Packet), SinkError = Error> + 'static,
> Sink for DefaultPacketHandlerSink<InnerSink> {
    type SinkItem = InnerSink::SinkItem;
    type SinkError = InnerSink::SinkError;

    fn start_send(
        &mut self,
        item: Self::SinkItem,
    ) -> futures::StartSend<Self::SinkItem, Self::SinkError> {
        // Check if it is a disconnect packet
        if let Packet { data: packets::Data::Command(ref command), .. } = item.1 {
            let cmd = command.get_commands().remove(0);
            if cmd.command == "clientdisconnect" {
                if let Some(data) = self.data.upgrade() {
                    let mut data = data.borrow_mut();
                    if let Some(con) = data.connection_manager.get_connection(
                        item.0.clone()) {
                        con.borrow_mut().resender.handle_event(
                            ResenderEvent::Disconnecting);
                    }
                }
            }
        }

        self.inner_sink.start_send(item)
    }

    fn poll_complete(&mut self) -> futures::Poll<(), Self::SinkError> {
        self.inner_sink.poll_complete()
    }

    fn close(&mut self) -> futures::Poll<(), Self::SinkError> {
        self.inner_sink.close()
    }
}*/
