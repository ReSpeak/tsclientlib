use std::cell::RefCell;
use std::mem;
use std::net::SocketAddr;
use std::rc::Rc;

use {tomcrypt, base64};
use chrono::Utc;
use futures::{self, future, Future, Sink, Stream};
use futures::future::Either;
use futures::task::{self, Task};
use futures::unsync::oneshot;
use num::{pow, BigUint, Integer, ToPrimitive};
use rand::{self, Rng};

use {packets, BoxFuture, Error, Result};
use algorithms as algs;
use commands::Command;
use handler_data::*;
use handler_data::Data;
use packets::*;

/// The data of our client.
pub type ClientData = Data<ServerConnectionData>;
/// Connection to a server from our client.
pub(crate) type ServerConnection = Connection<ServerConnectionData>;

pub struct ServerConnectionData {
    pub state_change_listener: Vec<Box<FnMut() -> BoxFuture<(), Error>>>,
    pub state: ServerConnectionState,
}

#[derive(Debug)]
pub enum ServerConnectionState {
    /// After `Init0` was sent.
    Init0 { version: u32, random0: [u8; 4] },
    /// After `Init2` was sent.
    Init2 { version: u32 },
    /// After `Init4` was sent.
    ClientInitIv {
        alpha: [u8; 10],
        private_key: tomcrypt::EccKey,
    },
    Connected,
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
pub fn default_setup(data: Rc<RefCell<ClientData>>) {
    // Packet encoding
    ::packet_codec::PacketCodecSink::apply(data.clone());
    ::packet_codec::PacketCodecStream::apply(data.clone(), true);
    // Logging
    ::log::apply_udp_packet_logger(data.clone());
    ::log::apply_packet_logger(data.clone());

    // Default handlers
    DefaultPacketHandler::apply(data.clone(), true);
}

/// Wait until a client is connected.
fn connect_wait(
    data: Rc<RefCell<ClientData>>,
    addr: SocketAddr,
) -> BoxFuture<(), Error> {
    // Return a future that resolves when we are connected
    let data2 = data.clone();
    if let Some(con) = data.borrow_mut().connections.get_mut(&addr) {
        if let ServerConnectionState::Connected = con.state.state {
            return Box::new(future::ok(()));
        }
        // Wait for the next state change
        let (send, recv) = oneshot::channel();
        let mut send = Some(send);
        con.state.state_change_listener.push(Box::new(move || {
            send.take().unwrap().send(()).unwrap();
            Box::new(future::ok(()))
        }));
        Box::new(
            recv.map_err(|e| e.into())
                .and_then(move |_| connect_wait(data2, addr)),
        )
    } else {
        Box::new(future::ok(()))
    }
}

/// Connect to a server
pub fn connect(
    data: Rc<RefCell<ClientData>>,
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
        timestamp: timestamp,
        random0,
    };

    let cheader = create_init_header();
    // Add the connection to the connection list
    {
        let mut data = data.borrow_mut();
        data.connections.insert(
            server_addr,
            ServerConnection::new(ServerConnectionData {
                state_change_listener: Vec::new(),
                state: ServerConnectionState::Init0 {
                    version: timestamp,
                    random0,
                },
            }),
        );
    }

    let packet = Packet::new(cheader, packets::Data::C2SInit(packet_data));
    Box::new(
        ClientData::get_packets(data.clone())
            .send((server_addr, packet))
            .and_then(move |_| connect_wait(data, server_addr)),
    )
}

/// Disconnect from a server
pub fn disconnect(
    data: Rc<RefCell<ClientData>>,
    server_addr: SocketAddr,
) -> BoxFuture<(), Error> {
    {
        // Check if we are connected to this server
        let data = data.borrow();
        if let Some(con) = data.connections.get(&server_addr) {
            if let ServerConnectionState::Connected = con.state.state {
            } else {
                return Box::new(future::err(
                    "Cannot disconnect from connection when not connected"
                        .into(),
                ));
            }
        } else {
            return Box::new(future::err(
                "Cannot disconnect from unknown connection".into(),
            ));
        }
    }
    let header = Header::new(PacketType::Command);
    let mut command = Command::new("clientdisconnect");
    // Never times out
    //let mut command = Command::new("clientinitiv");
    // Reason: Disconnect
    command.push("reasonid", "8");
    command.push("reasonmsg", "Bye");
    let p_data = packets::Data::Command(command);
    let packet = Packet::new(header, p_data);
    Box::new(
        ClientData::get_packets(data)
            .send((server_addr, packet))
            .map(|_| ()),
    )
}

pub struct DefaultPacketHandlerStream {
    inner_stream: Box<Stream<Item = (SocketAddr, Packet), Error = Error>>,
}

impl DefaultPacketHandlerStream {
    pub fn new<
        InnerStream: Stream<Item = (SocketAddr, Packet), Error = Error> + 'static,
        InnerSink: Sink<SinkItem = (SocketAddr, Packet), SinkError = Error> + 'static,
    >(
        data: Rc<RefCell<ClientData>>,
        inner_stream: InnerStream,
        inner_sink: InnerSink,
        send_clientinit: bool,
    ) -> (Self, Rc<RefCell<Either<InnerSink, Option<Task>>>>) {
        let sink = Rc::new(RefCell::new(Either::A(inner_sink)));
        let sink2 = sink.clone();
        let inner_stream = Box::new(inner_stream.and_then(move |(addr, packet)| -> BoxFuture<_, _> {
            // Check if we have a connection for this server
            let packet_res = {
                let mut data = data.borrow_mut();
                let logger = data.logger.clone();
                let data = &mut *data;
                if let Some(con) = data.connections.get_mut(&addr) {
                    let mut update_rtt = None;
                    let handle_res = match con.state.state {
                        ServerConnectionState::Init0 { version, ref random0 } => {
                            // Handle an Init1
                            if let Packet { data: packets::Data::S2CInit(
                                S2CInit::Init1 { ref random1, ref random0_r }), .. } = packet {
                                // Check the response
                                if random0.as_ref().iter().rev().eq(random0_r.as_ref()) {
                                    // The packet is correct.
                                    // Send next init packet
                                    let mut mac = [0; 8];
                                    mac.copy_from_slice(b"TS3INIT1");
                                    let cheader = create_init_header();
                                    let data = C2SInit::Init2 {
                                        version: version,
                                        random1: *random1,
                                        random0_r: *random0_r,
                                    };

                                    let state = ServerConnectionState::Init2 {
                                        version,
                                    };

                                    Some((state, Packet::new(cheader,
                                        packets::Data::C2SInit(data))))
                                } else {
                                    error!(logger, "Init: Got wrong data in the \
                                        Init1 response packet");
                                    None
                                }
                            } else {
                                None
                            }
                        }
                        ServerConnectionState::Init2 { version } => {
                            // Handle an Init3
                            if let Packet { data: packets::Data::S2CInit(
                                S2CInit::Init3 { ref x, ref n, level, ref random2 }), .. } = packet {
                                // Solve RSA puzzle: y = x ^ (2 ^ level) % n
                                // Use Montgomery Reduction
                                let xi = BigUint::from_bytes_be(x);
                                let ni = BigUint::from_bytes_be(n);
                                // TODO implement Montgomery Reduction, use another thread + timeout
                                fn pow_mod(mut x: BigUint, level: u32, n: &BigUint) -> BigUint {
                                    for _ in 0..level {
                                        x = pow::pow(x, 2).mod_floor(n);
                                    }
                                    x
                                }
                                let mut time_reporter = ::slog_perf::TimeReporter::new_with_level(
                                    "Solve RSA puzzle", logger.clone(),
                                    ::slog::Level::Info);
                                time_reporter.start("");
                                let yi = pow_mod(xi.clone(), level, &ni);
                                time_reporter.finish();
                                info!(logger, "Solve RSA puzzle";
                                      "level" => level, "x" => %xi, "n" => %ni,
                                      "y" => %yi);
                                let y = algs::biguint_to_array(&yi);

                                // Create ECDH key
                                //let prng = tomcrypt::sprng();
                                //let mut private_key = tryf!(tomcrypt::EccKey::new(prng, 32));
                                let mut private_key = tryf!(tomcrypt::EccKey::import(
                                    &base64::decode("MG0DAgeAAgEgAiAIXJBlj1hQbaH0Eq0DuLlCmH8bl+veTAO2+\
                                        k9EQjEYSgIgNnImcmKo7ls5mExb6skfK2Tw+u54aeDr0OP1ITsC/50CIA8M5nm\
                                        DBnmDM/gZ//4AAAAAAAAAAAAAAAAAAAAZRzOI").unwrap()));
                                let omega = tryf!(private_key.export_public());

                                // Create the command string
                                let mut rng = rand::thread_rng();
                                let alpha = rng.gen::<[u8; 10]>();
                                // omega is an ASN.1-DER encoded public key from the
                                // ECDH parameters.
                                let alpha_s = base64::encode(&alpha);
                                let omega_s = base64::encode(&omega);
                                let mut command = Command::new("clientinitiv");
                                command.push("alpha", alpha_s);
                                command.push("omega", omega_s);
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
                                    private_key,
                                };

                                Some((state, Packet::new(cheader,
                                    packets::Data::C2SInit(data))))
                            } else {
                                None
                            }
                        }
                        ServerConnectionState::ClientInitIv { ref alpha, ref mut private_key } => {
                            let res = (|con_params: &mut Option<ConnectedParams>| -> Result<()> {
                                if let Packet { data: packets::Data::Command(ref command), .. } = packet {
                                    let cmd = command.get_commands().remove(0);
                                    if cmd.command != "initivexpand"
                                        || !cmd.has_arg("alpha")
                                        || !cmd.has_arg("beta")
                                        || !cmd.has_arg("omega")
                                        || base64::decode(cmd.args["alpha"])
                                        .map(|a| a != alpha).unwrap_or(true) {
                                        bail!("initivexpand command has wrong arguments");
                                    } else {
                                        let beta_vec = base64::decode(cmd.args["beta"])?;
                                        let omega = base64::decode(cmd.args["omega"])?;
                                        let mut beta = [0; 10];
                                        beta.copy_from_slice(&beta_vec);
                                        let mut server_key = tomcrypt::EccKey::import(&omega)?;

                                        let (iv, mac) = algs::compute_iv_mac(
                                            alpha, &beta, private_key, &mut server_key)?;
                                        let mut params = ConnectedParams::new(
                                            iv, mac);
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
                                    }
                                    Ok(())
                                } else {
                                    Ok(())
                                }})(&mut con.params);
                            if let Err(error) = res {
                                error!(logger, "Handle udp init packet"; "error" => ?error);
                                None
                            } else {
                                // Compute hash cash
                                let mut time_reporter = ::slog_perf::TimeReporter::new_with_level(
                                    "Compute public key hash cash level", logger.clone(),
                                    ::slog::Level::Info);
                                time_reporter.start("Compute public key hash cash level");
                                let offset = algs::hash_cash(private_key, 8).unwrap();
                                let omega = base64::encode(&private_key.export_public().unwrap());
                                time_reporter.finish();
                                info!(logger, "Computed hash cash level";
                                    "level" => algs::get_hash_cash_level(&omega, offset),
                                    "offset" => offset);

                                // TODO Don't create clientinit packet in this library
                                let header = Header::new(PacketType::Command);
                                let mut command = Command::new("clientinit");
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

                                Some((ServerConnectionState::Connected, clientinit_packet))
                            }
                        }
                        ServerConnectionState::Connected => {
                            // Handle an initserver
                            if let Packet { data: packets::Data::Command(ref cmd), .. } = packet {
                                let cmd = cmd.get_commands().remove(0);
                                if cmd.command == "initserver" && cmd.has_arg("aclid") {
                                    if let Some(ref mut params) = con.params {
                                        if let Ok(c_id) = cmd.args["aclid"].parse() {
                                            params.c_id = c_id;
                                        }
                                    }
                                    // initserver is the ack for clientinit
                                    // Remove from send queue
                                    let p_type = PacketType::Command;
                                    let p_id = 1;
                                    let mut rec = None;
                                    let mut items = data.send_queue.drain()
                                        .filter_map(|r| if r.p_type == p_type && r.p_id == p_id {
                                            rec = Some(r);
                                            None
                                        } else {
                                            Some(r)
                                        })
                                        .collect();
                                    mem::swap(&mut items, &mut data.send_queue);
                                    // Update smoothed round trip time
                                    if let Some(rec) = rec {
                                        // Only if it was not resent
                                        if rec.tries == 1 {
                                            let now = Utc::now();
                                            let diff = now.naive_utc().signed_duration_since(rec.sent.naive_utc());
                                            update_rtt = Some(diff);
                                        }
                                    }
                                }
                            }
                            None
                        }
                    };
                    if let Some((state, packet)) = handle_res {
                        if let Some(rtt) = update_rtt {
                            con.update_srtt(rtt);
                        }
                        con.state.state = state;
                        let mut listeners = Vec::new();
                        mem::swap(&mut listeners, &mut con.state.state_change_listener);
                        Some((listeners, packet))
                    } else {
                        None
                    }
                } else {
                    None
                }
            };
            if let Some((mut listeners, p)) = packet_res {
                // Notify state changed listeners
                let l_fut = future::join_all(listeners.drain(..).map(|mut l| l()).collect::<Vec<_>>());

                if let packets::Data::Command(ref cmd) = p.data {
                    if cmd.command == "clientinit" {
                        if !send_clientinit {
                            return Box::new(l_fut.and_then(|_| future::ok(None)));
                        }
                    }
                }
                // Take sink
                let mut tmp_sink = Either::B(None);
                mem::swap(&mut tmp_sink, &mut *sink.borrow_mut());
                let tmp_sink = if let Either::A(sink) = tmp_sink {
                    sink
                } else {
                    unreachable!("Sink is not available");
                };
                // Send the packet
                let sink = sink.clone();
                Box::new(l_fut.and_then(move |_| tmp_sink.send((addr, p)).map(move |tmp_sink| {
                    let mut s: Either<InnerSink, Option<Task>> = Either::A(tmp_sink);
                    mem::swap(&mut s, &mut *sink.borrow_mut());
                    if let Either::B(Some(task)) = s {
                        // Notify the task, that the sink is available
                        task.notify();
                    }
                    // Already handled
                    None
                })))
            } else {
                Box::new(future::ok(Some((addr, packet))))
            }
        }).filter_map(|o| o));
        (Self { inner_stream }, sink2)
    }
}

impl Stream for DefaultPacketHandlerStream {
    type Item = (SocketAddr, Packet);
    type Error = Error;

    fn poll(&mut self) -> futures::Poll<Option<Self::Item>, Self::Error> {
        self.inner_stream.poll()
    }
}

pub struct DefaultPacketHandlerSink<
    InnerSink: Sink<SinkItem = (SocketAddr, Packet), SinkError = Error> + 'static,
> {
    inner_sink: Rc<RefCell<Either<InnerSink, Option<Task>>>>,
}

impl<
    InnerSink: Sink<SinkItem = (SocketAddr, Packet), SinkError = Error> + 'static,
> DefaultPacketHandlerSink<InnerSink> {
    pub fn new(
        inner_sink: Rc<RefCell<Either<InnerSink, Option<Task>>>>,
    ) -> Self {
        Self { inner_sink }
    }
}

impl<
    InnerSink: Sink<SinkItem = (SocketAddr, Packet), SinkError = Error> + 'static,
> Sink for DefaultPacketHandlerSink<InnerSink> {
    type SinkItem = InnerSink::SinkItem;
    type SinkError = InnerSink::SinkError;

    fn start_send(
        &mut self,
        item: Self::SinkItem,
    ) -> futures::StartSend<Self::SinkItem, Self::SinkError> {
        let mut sink = self.inner_sink.borrow_mut();
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
    InnerSink: Sink<SinkItem = (SocketAddr, Packet), SinkError = Error> + 'static,
> {
    inner_stream: DefaultPacketHandlerStream,
    inner_sink: DefaultPacketHandlerSink<InnerSink>,
}

impl<
    InnerSink: Sink<SinkItem = (SocketAddr, Packet), SinkError = Error> + 'static,
> DefaultPacketHandler<InnerSink> {
    pub fn new<
        InnerStream: Stream<Item = (SocketAddr, Packet), Error = Error> + 'static,
    >(
        data: Rc<RefCell<ClientData>>,
        inner_stream: InnerStream,
        inner_sink: InnerSink,
        send_clientinit: bool,
    ) -> Self {
        let (inner_stream, inner_sink) = DefaultPacketHandlerStream::new(
            data,
            inner_stream,
            inner_sink,
            send_clientinit,
        );
        let inner_sink = DefaultPacketHandlerSink::new(inner_sink);
        Self {
            inner_stream,
            inner_sink,
        }
    }

    pub fn split(
        self,
    ) -> (
        DefaultPacketHandlerSink<InnerSink>,
        DefaultPacketHandlerStream,
    ) {
        (self.inner_sink, self.inner_stream)
    }
}

impl
    DefaultPacketHandler<
        Box<Sink<SinkItem = (SocketAddr, Packet), SinkError = Error>>,
    > {
    pub fn apply(data: Rc<RefCell<ClientData>>, send_clientinit: bool) {
        let data2 = data.clone();
        let mut data = data.borrow_mut();
        let handler = Self::new(
            data2.clone(),
            data.packet_stream.take().unwrap(),
            data.packet_sink.take().unwrap(),
            send_clientinit,
        );
        let (sink, stream) = handler.split();
        data.packet_stream = Some(Box::new(stream));
        data.packet_sink = Some(Box::new(sink));
    }
}
