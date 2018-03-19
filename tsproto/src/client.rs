use std::cell::RefCell;
use std::fmt::Display;
use std::mem;
use std::net::SocketAddr;
use std::rc::Rc;

use base64;
use chrono::Utc;
use futures::{self, future, Future, Sink, Stream};
use futures::future::Either;
use futures::task::{self, Task};
use futures::unsync::oneshot;
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
use connectionmanager::{AttachedDataConnectionManager, Resender, ResenderEvent,
    SocketConnectionManager};
use crypto::{EccKeyPrivP256, EccKeyPubP256, EccKeyPrivEd25519, EccKeyPubEd25519};
use handler_data::Data;
use license::Licenses;
use packets::*;

/// The data of our client.
pub type ClientData = Data<SocketConnectionManager<ServerConnectionData>>;
/// Connections from a client to a server.
pub type ClientConnection = Connection<SocketConnectionManager<ServerConnectionData>>;

#[derive(Default)]
pub struct ServerConnectionData {
    pub state_change_listener: Vec<Box<FnMut() -> BoxFuture<(), Error>>>,
    pub state: ServerConnectionState,
}

#[derive(Debug)]
pub enum ServerConnectionState {
    /// Default state.
    Uninitialized,
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

impl Default for ServerConnectionState {
    fn default() -> Self {
        ServerConnectionState::Uninitialized
    }
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
pub fn default_setup<
    CM: AttachedDataConnectionManager<ServerConnectionData> + 'static,
>(data: &Rc<RefCell<Data<CM>>>, log: bool) where CM::ConnectionsKey: Display {
    if log {
        // Logging
        let a = {
            let data = data.borrow();
            (data.logger.clone(), data.is_client)
        };
        Data::apply_udp_packet_stream_wrapper::<
            ::log::UdpPacketStreamLogger>(data, a.clone());
        Data::apply_udp_packet_sink_wrapper::<
            ::log::UdpPacketSinkLogger>(data, a.clone());

        Data::apply_packet_stream_wrapper::<
            ::log::PacketStreamLogger<CM::ConnectionsKey>>(data, a.clone());
        Data::apply_packet_sink_wrapper::<
            ::log::PacketSinkLogger<CM::ConnectionsKey>>(data, a);
    }

    DefaultPacketHandler::apply(data);
}

/// Wait until a client reaches a certain state.
///
/// `is_state` should return `true`, if the state is reached and `false` if this
/// function should continue waiting.
pub fn wait_for_state<F: Fn(&ServerConnectionState) -> bool + 'static>(
    data: &Rc<RefCell<ClientData>>,
    server_addr: SocketAddr,
    f: F,
) -> BoxFuture<(), Error> {
    // Return a future that resolves when the function succeeds
    let data2 = data.clone();
    if let Some(state) = data.borrow_mut().connection_manager
        .get_mut_data(server_addr) {
        if f(&state.state) {
            return Box::new(future::ok(()));
        }
        // Wait for the next state change
        let (send, recv) = oneshot::channel();
        let mut send = Some(send);
        state.state_change_listener.push(Box::new(move || {
            send.take().unwrap().send(()).unwrap();
            Box::new(future::ok(()))
        }));
        Box::new(
            recv.map_err(|e| e.into())
                .and_then(move |_| wait_for_state(&data2, server_addr, f)),
        )
    } else {
        Box::new(future::ok(()))
    }
}

pub fn wait_until_connected(
    data: &Rc<RefCell<ClientData>>,
    server_addr: SocketAddr,
) -> BoxFuture<(), Error> {
    wait_for_state(data, server_addr, |state| {
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
    data: &Rc<RefCell<ClientData>>,
    server_addr: SocketAddr,
) -> BoxFuture<(), Error> {
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
    // Add the connection to the connection list
    Data::add_connection(data, Data::create_connection(data, server_addr));

    // Change the state
    let data2 = data.clone();
    let mut data = data.borrow_mut();
    {
        let con_data = data.connection_manager.get_mut_data(server_addr)
            .unwrap();
        con_data.state = ServerConnectionState::Init0 {
            version: timestamp,
            random0,
        };
    }

    let packets = Data::get_packets(Rc::downgrade(&data2));

    let packet = Packet::new(cheader, packets::Data::C2SInit(packet_data));
    Box::new(
        packets.send((server_addr, packet))
            .and_then(move |_| {
                wait_for_state(&data2, server_addr, |state| {
                    if let ServerConnectionState::Connecting = *state {
                        true
                    } else {
                        false
                    }
                })
            }),
    )
}

struct DefaultPacketHandlerStream;

impl DefaultPacketHandlerStream {
    pub fn new<
        CM: AttachedDataConnectionManager<ServerConnectionData> + 'static,
        InnerStream: Stream<Item = (CM::ConnectionsKey, Packet), Error = Error> + 'static,
        InnerSink: Sink<SinkItem = (CM::ConnectionsKey, Packet), SinkError = Error> + 'static,
    >(
        data: &Rc<RefCell<Data<CM>>>,
        inner_stream: InnerStream,
        inner_sink: InnerSink,
    ) -> (Box<Stream<Item = (CM::ConnectionsKey, Packet), Error = Error>>,
        Rc<RefCell<Either<InnerSink, Option<Task>>>>) {
        let sink = Rc::new(RefCell::new(Either::A(inner_sink)));
        let sink2 = sink.clone();
        let logger = data.borrow().logger.clone();
        let data = Rc::downgrade(data);
        let inner_stream = Box::new(inner_stream.and_then(move |(key, packet)| -> BoxFuture<_, _> {
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
                    let state = data.connection_manager
                        .get_mut_data(key.clone()).unwrap();
                    let handle_res = match Self::handle_packet(state, &packet,
                        &mut ignore_packet, &mut is_end, &data.private_key,
                        &mut con, &logger) {
                        Ok(res) => res,
                        Err(error) => {
                            error!(logger, "Error when handling packet";
                                "error" => ?error);
                            None
                        }
                    };
                    if let Some((s, packet)) = handle_res {
                        state.state = s;
                        let listeners = mem::replace(&mut state.state_change_listener, Vec::new());
                        Some((listeners, packet))
                    } else {
                        None
                    }
                } else {
                    None
                }
            };

            if is_end {
                Data::remove_connection(&data.upgrade().unwrap(), key.clone());
            }

            if let Some((mut listeners, p)) = packet_res {
                // First send the packet, then notify the listeners, this
                // ensures that the clientek packet is sent before the
                // clientinit.
                let res_fut: BoxFuture<(), _> = if let Some(p) = p {
                    // Take sink
                    let tmp_sink = mem::replace(&mut *sink.borrow_mut(), Either::B(None));
                    let tmp_sink = if let Either::A(sink) = tmp_sink {
                        sink
                    } else {
                        unreachable!("Sink is not available");
                    };
                    // Send the packet
                    let sink = sink.clone();
                    Box::new(tmp_sink.send((key.clone(), p))
                            .map(move |tmp_sink| {
                        let s: Either<InnerSink, Option<Task>> =
                            mem::replace(&mut *sink.borrow_mut(), Either::A(tmp_sink));
                        if let Either::B(Some(task)) = s {
                            // Notify the task, that the sink is available
                            task.notify();
                        }
                    }))
                } else {
                    Box::new(future::ok(()))
                };

                Box::new(res_fut.and_then(move |_| {
                    // Notify state changed listeners
                    let listeners = listeners.drain(..).map(|mut l| l())
                        .collect::<Vec<_>>();
                    future::join_all(listeners)
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
        CM: AttachedDataConnectionManager<ServerConnectionData> + 'static,
    >(state: &mut ServerConnectionData, packet: &Packet,
        ignore_packet: &mut bool, is_end: &mut bool,
        private_key: &EccKeyPrivP256, con: &mut Connection<CM>,
        logger: &Logger)
        -> Result<Option<(ServerConnectionState, Option<Packet>)>> {
        let res = match state.state {
            ServerConnectionState::Uninitialized =>
                return Err(format_err!("ServerConnectionState is uninitialized").into()),
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
                        // TODO use another thread
                        let mut time_reporter = ::slog_perf::TimeReporter::new_with_level(
                            "Solve RSA puzzle", logger.clone(),
                            ::slog::Level::Info);
                        time_reporter.start("");

                        // Use gmp for faster computations if it is
                        // available.
                        #[cfg(feature = "rust-gmp")]
                        let y = {
                            let n = (n as &[u8]).into();
                            let x: Mpz = (x as &[u8]).into();
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
                            let xi = BigUint::from_bytes_be(x);
                            let ni = BigUint::from_bytes_be(n);
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
                        let mut rng = rand::thread_rng();
                        let alpha = rng.gen::<[u8; 10]>();
                        // omega is an ASN.1-DER encoded public key from
                        // the ECDH parameters.
                        let alpha_s = base64::encode(&alpha);
                        let omega_s = private_key.to_pub().to_ts().unwrap();
                        let mut command = Command::new("clientinitiv");
                        command.push("alpha", alpha_s);
                        command.push("omega", omega_s);
                        command.push("ot", "1");
                        command.push("ip", "");

                        let cheader = create_init_header();
                        let data = C2SInit::Init4 {
                            version,
                            x: *x,
                            n: *n,
                            level,
                            random2: *random2,
                            y,
                            command: command.clone(),
                        };

                        let state = ServerConnectionState::ClientInitIv {
                            alpha,
                        };

                        Some((state, Some(Packet::new(cheader,
                            packets::Data::C2SInit(data)))))
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
                            // Check signature of l (proof)
                            // TODO

                            let beta_vec = base64::decode(cmd.args["beta"])?;
                            if beta_vec.len() != 54 {
                                return Err(format_err!(
                                    "Incorrect beta length").into());
                            }

                            let mut beta = [0; 54];
                            beta.copy_from_slice(&beta_vec);

                            // Parse license argument
                            let licenses = Licenses::parse(&base64::decode(
                                cmd.args["l"])?)?;
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
                            let ek_pub: EccKeyPubEd25519 = (&ek).into();
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
    CM: AttachedDataConnectionManager<ServerConnectionData> + 'static,
    InnerSink: Sink<SinkItem = (CM::ConnectionsKey, Packet), SinkError = Error> + 'static,
> {
    inner_sink: Rc<RefCell<Either<InnerSink, Option<Task>>>>,
    phantom: ::std::marker::PhantomData<CM>,
}

impl<
    CM: AttachedDataConnectionManager<ServerConnectionData> + 'static,
    InnerSink: Sink<SinkItem = (CM::ConnectionsKey, Packet), SinkError = Error> + 'static,
> DefaultPacketHandlerSink<CM, InnerSink> {
    pub fn new(
        inner_sink: Rc<RefCell<Either<InnerSink, Option<Task>>>>,
    ) -> Self {
        Self { inner_sink, phantom: ::std::marker::PhantomData }
    }
}

impl<
    CM: AttachedDataConnectionManager<ServerConnectionData> + 'static,
    InnerSink: Sink<SinkItem = (CM::ConnectionsKey, Packet), SinkError = Error> + 'static,
> Sink for DefaultPacketHandlerSink<CM, InnerSink> {
    type SinkItem = InnerSink::SinkItem;
    type SinkError = InnerSink::SinkError;

    fn start_send(
        &mut self,
        item: Self::SinkItem,
    ) -> futures::StartSend<Self::SinkItem, Self::SinkError> {
        let mut sink = self.inner_sink.borrow_mut();
        // TODO Send ResenderEvent::Disconnecting here
        if let Either::A(ref mut sink) = *sink {
            return sink.start_send(item);
        }
        *sink = Either::B(Some(task::current()));
        Ok(futures::AsyncSink::NotReady(item))
    }

    fn poll_complete(&mut self) -> futures::Poll<(), Self::SinkError> {
        let mut sink = self.inner_sink.borrow_mut();
        if let Either::A(ref mut sink) = *sink {
            return sink.poll_complete();
        }
        *sink = Either::B(Some(task::current()));
        Ok(futures::Async::NotReady)
    }

    fn close(&mut self) -> futures::Poll<(), Self::SinkError> {
        let mut sink = self.inner_sink.borrow_mut();
        if let Either::A(ref mut sink) = *sink {
            return sink.close();
        }
        *sink = Either::B(Some(task::current()));
        Ok(futures::Async::NotReady)
    }
}

pub struct DefaultPacketHandler<
    CM: AttachedDataConnectionManager<ServerConnectionData> + 'static,
    InnerSink: Sink<SinkItem = (CM::ConnectionsKey, Packet), SinkError = Error> + 'static,
> {
    inner_stream: Box<Stream<Item = (CM::ConnectionsKey, Packet), Error = Error>>,
    inner_sink: DefaultPacketHandlerSink<CM, InnerSink>,
}

impl<
    CM: AttachedDataConnectionManager<ServerConnectionData> + 'static,
    InnerSink: Sink<SinkItem = (CM::ConnectionsKey, Packet), SinkError = Error> + 'static,
> DefaultPacketHandler<CM, InnerSink> {
    pub fn new<
        InnerStream: Stream<Item = (CM::ConnectionsKey, Packet), Error = Error> + 'static,
    >(
        data: &Rc<RefCell<Data<CM>>>,
        inner_stream: InnerStream,
        inner_sink: InnerSink,
    ) -> Self {
        let (inner_stream, inner_sink) = DefaultPacketHandlerStream::new(
            data,
            inner_stream,
            inner_sink,
        );
        let inner_sink = DefaultPacketHandlerSink::new(inner_sink);
        Self {
            inner_stream,
            inner_sink,
        }
    }

    pub fn split(self) -> (
        DefaultPacketHandlerSink<CM, InnerSink>,
        Box<Stream<Item = (CM::ConnectionsKey, Packet), Error = Error>>,
    ) {
        (self.inner_sink, self.inner_stream)
    }
}

impl<
    CM: AttachedDataConnectionManager<ServerConnectionData> + 'static,
> DefaultPacketHandler<CM, Box<Sink<SinkItem = (CM::ConnectionsKey, Packet),
    SinkError = Error>>> {
    pub fn apply(data: &Rc<RefCell<Data<CM>>>) {
        let (stream, sink) = {
            let mut data = data.borrow_mut();
            (data.packet_stream.take().unwrap(),
                data.packet_sink.take().unwrap())
        };
        let handler = Self::new(data, stream, sink);
        let (sink, stream) = handler.split();
        let mut data = data.borrow_mut();
        data.packet_stream = Some(Box::new(stream));
        data.packet_sink = Some(Box::new(sink));
    }
}
