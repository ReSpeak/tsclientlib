use std::net::SocketAddr;
use std::sync::Weak;

use chrono::Utc;
use futures::sync::mpsc;
use futures::{future, Future, Sink, Stream};
#[cfg(not(feature = "rug"))]
use num::bigint::BigUint;
#[cfg(not(feature = "rug"))]
use num::One;
use parking_lot::Mutex;
use rand::{self, Rng};
#[cfg(feature = "rug")]
use rug::integer::Order;
#[cfg(feature = "rug")]
use rug::Integer;
use slog::{error, info, warn, Logger};
use {base64, tokio, tokio_threadpool};

use crate::algorithms as algs;
use crate::commands::Command;
use crate::connection::*;
use crate::connectionmanager::{
	Resender, ResenderEvent, SocketConnectionManager,
};
use crate::crypto::{EccKeyPrivEd25519, EccKeyPrivP256, EccKeyPubP256};
use crate::handler_data::{
	ConnectionValue, ConnectionValueWeak, Data, DataM, PacketHandler,
};
use crate::license::Licenses;
use crate::packets::*;
use crate::{packets, Error, Result};

pub type CM<PH> =
	SocketConnectionManager<DefaultPacketHandler<PH>, ServerConnectionData>;
/// The data of our client.
pub type ClientData<PH> = Data<CM<PH>>;
pub type ClientDataM<PH> = DataM<CM<PH>>;
/// Connections from a client to a server.
pub type ClientConnection = Connection;
pub type ClientConVal = ConnectionValueWeak<ServerConnectionData>;

pub struct ServerConnectionData {
	/// Every function in this list is called when the state of the connection
	/// changes.
	///
	/// Return `false` to remain in the list of listeners.
	/// If `true` is returned, this listener will be removed.
	pub state_change_listener:
		Vec<Box<FnMut(&ServerConnectionState) -> bool + Send>>,
	pub state: ServerConnectionState,
}

#[derive(Debug)]
pub enum ServerConnectionState {
	/// After `Init0` was sent.
	Init0 { version: u32 },
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

/// Wait until a client reaches a certain state.
///
/// `is_state` should return `true`, if the state is reached and `false` if this
/// function should continue waiting.
pub fn wait_for_state<
	F: Fn(&ServerConnectionState) -> bool + Send + 'static,
>(
	connection: &ClientConVal,
	f: F,
) -> Box<Future<Item = (), Error = Error> + Send>
{
	let (send, recv) = mpsc::channel(0);
	let con = match connection.mutex.upgrade() {
		Some(c) => c,
		None => {
			return Box::new(future::err(
				format_err!("Connection is gone").into(),
			));
		}
	};
	let mut con = con.lock();
	con.0.state_change_listener.push(Box::new(move |s| {
		// Check if it is the right state
		if f(s) {
			let send = send.clone();
			// Ignore errors
			tokio::spawn(send.send(()).then(|_| Ok(())));
			true
		} else {
			false
		}
	}));
	Box::new(recv.into_future().map(|_| ()).map_err(|e| {
		format_err!("Failed to receive while waiting for state ({:?})", e)
			.into()
	}))
}

pub fn wait_until_connected(
	connection: &ClientConVal,
) -> impl Future<Item = (), Error = Error> {
	wait_for_state(connection, |state| {
		if let ServerConnectionState::Connected = state {
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
/// [`ServerConnectionState::Connecting`]: enum.ServerConnectionState.html
/// [`wait_until_connected`]: method.wait_until_connected.html
pub fn connect<PH: PacketHandler<ServerConnectionData>>(
	datam: Weak<Mutex<ClientData<PH>>>,
	data: &mut ClientData<PH>,
	server_addr: SocketAddr,
) -> impl Future<Item = ClientConVal, Error = Error>
{
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
		state: ServerConnectionState::Init0 { version: timestamp },
	};
	// Add the connection to the connection list
	let key = data.add_connection(datam, state, server_addr);
	let con = data.get_connection(&key).unwrap().downgrade();

	let packet = Packet::new(cheader, packets::Data::C2SInit(packet_data));
	let con2 = con.clone();
	con.as_packet_sink()
		.send(packet)
		.and_then(move |_| {
			wait_for_state(&con, |state| {
				if let ServerConnectionState::Connecting = *state {
					true
				} else {
					false
				}
			})
		})
		.and_then(move |_| Ok(con2))
}

pub struct DefaultPacketHandler<
	IPH: PacketHandler<ServerConnectionData> + 'static,
> {
	inner: IPH,
	/// The data instance is created after the packet handler so this has to be
	/// an option.
	data: Option<Weak<Mutex<ClientData<IPH>>>>,
}

impl<IPH: PacketHandler<ServerConnectionData> + 'static>
	PacketHandler<ServerConnectionData> for DefaultPacketHandler<IPH>
{
	fn new_connection<S1, S2, S3, S4>(
		&mut self,
		con_val: &ConnectionValue<ServerConnectionData>,
		s2c_init_stream: S1,
		c2s_init_stream: S2,
		command_stream: S3,
		audio_stream: S4,
	) where
		S1: Stream<Item = InS2CInit, Error = Error> + Send + 'static,
		S2: Stream<Item = InC2SInit, Error = Error> + Send + 'static,
		S3: Stream<Item = InCommand, Error = Error> + Send + 'static,
		S4: Stream<Item = InAudio, Error = Error> + Send + 'static,
	{
		let con_val2 = con_val.downgrade();
		let data = self.data.as_ref().unwrap().clone();
		let s2c_init_stream = s2c_init_stream
			.and_then(move |p| -> Result<Option<InS2CInit>> {
				// Get private key
				let key = {
					let d = if let Some(d) = data.upgrade() {
						d
					} else {
						// Connection doesn't exist anymore
						return Err(format_err!(
							"Connection does not exist while handling packet"
						)
						.into());
					};
					let d = d.lock();
					d.private_key.clone()
				};

				let con_val_weak = con_val2.clone();
				let con_val = con_val2
					.upgrade()
					.ok_or_else(|| format_err!("Connection is gone"))?;
				let con_val3 = con_val.clone();
				let mut con = con_val.mutex.lock();
				let logger = con.1.logger.clone();
				let mut ignore_packet = true;
				let mut is_end = false;
				let handle_res = match Self::handle_init(
					&con_val3,
					&mut *con,
					&p,
					&mut ignore_packet,
					&mut is_end,
					key,
					&logger,
				) {
					Ok(r) => r,
					Err(e) => {
						error!(logger, "Error handling client command";
							"error" => ?e);
						// Ignore packet, it is probably malformed
						return Ok(None);
					}
				};

				if let Some((s, packet)) = handle_res {
					con.0.state = s;
					if let Some(packet) = packet {
						// First send the packet, then notify the listeners,
						// this ensures that the clientek packet is sent
						// before the clientinit.
						tokio::spawn(
							con_val2
								.as_packet_sink()
								.send(packet)
								.and_then(move |_| {
									let mutex = con_val_weak
										.upgrade()
										.ok_or_else(|| {
											format_err!("Connection is gone")
										})?
										.mutex;
									let mut con = mutex.lock();
									let state = &mut con.0;
									// Notify state changed listeners
									let mut i = 0;
									while i < state.state_change_listener.len()
									{
										if (&mut state.state_change_listener[i])(
											&state.state,
										) {
											state
												.state_change_listener
												.remove(i);
										} else {
											i += 1;
										}
									}
									Ok(())
								})
								.map_err(move |e| {
									error!(logger,
										"Error sending response packet";
										"error" => ?e)
								}),
						);
					} else {
						let state = &mut con.0;
						// Notify state changed listeners
						let mut i = 0;
						while i < state.state_change_listener.len() {
							if (&mut state.state_change_listener[i])(
								&state.state,
							) {
								state.state_change_listener.remove(i);
							} else {
								i += 1;
							}
						}
					}
				}

				if is_end {
					// Close connection
					let addr = con.1.address;
					let d = if let Some(d) = data.upgrade() {
						d
					} else {
						// Connection doesn't exist anymore, ignore it, the
						// connection is already gone.
						return Ok(None);
					};
					d.lock().remove_connection(&addr);
				}

				if ignore_packet {
					Ok(None)
				} else {
					Ok(Some(p))
				}
			})
			.filter_map(|p| p);

		let con_val2 = con_val.downgrade();
		let data = self.data.as_ref().unwrap().clone();
		let command_stream = command_stream
			.and_then(move |cmd| -> Result<Option<InCommand>> {
				// Check if we handle this packet
				let name = cmd.name();
				if name != "initivexpand"
					&& name != "initivexpand2"
					&& name != "initserver"
					&& name != "notifyclientleftview"
					&& name != "notifyplugincmd"
				{
					// Forward packet
					return Ok(Some(cmd));
				}

				// Get private key
				let key = {
					let d = if let Some(d) = data.upgrade() {
						d
					} else {
						// Connection doesn't exist anymore
						return Err(format_err!(
							"Connection does not exist while handling packet"
						)
						.into());
					};
					let d = d.lock();
					d.private_key.clone()
				};

				let con_val_weak = con_val2.clone();
				let con_val = con_val2
					.upgrade()
					.ok_or_else(|| format_err!("Connection is gone"))?;
				let mut con = con_val.mutex.lock();
				let logger = con.1.logger.clone();
				let mut ignore_packet = true;
				let mut is_end = false;
				let handle_res = match Self::handle_command(
					&mut *con,
					&cmd,
					&mut ignore_packet,
					&mut is_end,
					key,
					&logger,
				) {
					Ok(r) => r,
					Err(e) => {
						error!(logger, "Error handling client command";
							"error" => ?e);
						// Ignore packet, it is probably malformed
						return Ok(None);
					}
				};

				if let Some((s, packet)) = handle_res {
					con.0.state = s;
					if let Some(packet) = packet {
						// First send the packet, then notify the listeners,
						// this ensures that the clientek packet is sent
						// before the clientinit.
						tokio::spawn(
							con_val2
								.as_packet_sink()
								.send(packet)
								.and_then(move |_| {
									let mutex = con_val_weak
										.upgrade()
										.ok_or_else(|| {
											format_err!("Connection is gone")
										})?
										.mutex;
									let mut con = mutex.lock();
									let state = &mut con.0;
									// Notify state changed listeners
									let mut i = 0;
									while i < state.state_change_listener.len()
									{
										if (&mut state.state_change_listener[i])(
											&state.state,
										) {
											state
												.state_change_listener
												.remove(i);
										} else {
											i += 1;
										}
									}
									Ok(())
								})
								.map_err(move |e| {
									error!(logger,
										"Error sending response packet";
										"error" => ?e)
								}),
						);
					} else {
						let state = &mut con.0;
						// Notify state changed listeners
						let mut i = 0;
						while i < state.state_change_listener.len() {
							if (&mut state.state_change_listener[i])(
								&state.state,
							) {
								state.state_change_listener.remove(i);
							} else {
								i += 1;
							}
						}
					}
				}

				if is_end {
					// Close connection
					let addr = con.1.address;
					let d = if let Some(d) = data.upgrade() {
						d
					} else {
						// Connection doesn't exist anymore, ignore it, the
						// connection is already gone.
						return Ok(None);
					};
					d.lock().remove_connection(&addr);
				}

				if ignore_packet {
					Ok(None)
				} else {
					Ok(Some(cmd))
				}
			})
			.filter_map(|p| p);

		self.inner.new_connection(
			con_val,
			s2c_init_stream,
			c2s_init_stream,
			command_stream,
			audio_stream,
		);
	}
}

impl<IPH: PacketHandler<ServerConnectionData> + 'static>
	DefaultPacketHandler<IPH>
{
	/// Do not forget to call [`complete`] afterwards.
	///
	/// [`complete`]: #method.complete
	pub fn new(inner: IPH) -> Self { Self { inner, data: None } }

	/// Needs to be called to complete the initialization of this packet handler.
	pub fn complete(&mut self, data: Weak<Mutex<ClientData<IPH>>>) {
		self.data = Some(data);
	}

	fn handle_init(
		con_value: &ConnectionValue<ServerConnectionData>,
		(state, con): &mut (ServerConnectionData, Connection),
		packet: &InS2CInit,
		ignore_packet: &mut bool,
		is_end: &mut bool,
		private_key: EccKeyPrivP256,
		logger: &Logger,
	) -> Result<Option<(ServerConnectionState, Option<Packet>)>>
	{
		let con_value = con_value.downgrade();
		let res = match state.state {
			ServerConnectionState::Init0 { version } => {
				// Handle an Init1
				packet.with_data(|init| {
					if let S2CInitData::Init1 { random1, random0_r } = init {
						// Check the response
						// Most of the time, random0_r is the reversed random0, but
						// sometimes it isn't so do not check it.
						// random0.as_ref().iter().rev().eq(random0_r.as_ref())
						con.resender.ack_packet(PacketType::Init, 0);
						// The packet is correct.
						// Send next init packet
						let cheader = create_init_header();
						let data = C2SInit::Init2 {
							version,
							random1: **random1,
							random0_r: **random0_r,
						};

						let state = ServerConnectionState::Init2 { version };

						Some((
							state,
							Some(Packet::new(
								cheader,
								packets::Data::C2SInit(data),
							)),
						))
					} else {
						None
					}
				})
			}
			ServerConnectionState::Init2 { version } => {
				// Handle an Init3
				packet.with_data(|init| {
					if let S2CInitData::Init3 {
						x,
						n,
						level,
						random2,
					} = init
					{
						let level = *level;
						// Solve RSA puzzle: y = x ^ (2 ^ level) % n
						// Use Montgomery Reduction
						if level > 10_000_000 {
							// Reject too high exponents
							None
						} else {
							con.resender.ack_packet(PacketType::Init, 2);
							// Create clientinitiv
							let mut rng = rand::thread_rng();
							let alpha = rng.gen::<[u8; 10]>();
							// omega is an ASN.1-DER encoded public key from
							// the ECDH parameters.

							let alpha_s = base64::encode(&alpha);
							let ip = con.address.ip();
							let logger = logger.clone();

							let x = **x;
							let n = **n;
							let random2 = **random2;

							// Spawn this as another future
							let logger2 = logger.clone();
							let fut = future::lazy(move || {
								let logger2 = logger.clone();
								future::poll_fn(move || {
									let logger = logger2.clone();
									tokio_threadpool::blocking(|| {
										let mut time_reporter = ::slog_perf::TimeReporter::new_with_level(
											"Solve RSA puzzle", logger.clone(),
											::slog::Level::Info);
										time_reporter.start("");

										// Use gmp for faster computations if it is
										// available.
										#[cfg(feature = "rug")]
										let y = {
											let mut e = Integer::new();
											let n = Integer::from_digits(
												&n[..],
												Order::Msf,
											);
											let x = Integer::from_digits(
												&x[..],
												Order::Msf,
											);
											e.set_bit(level, true);
											let y = match x.pow_mod(&e, &n) {
												Ok(r) => r,
												Err(_) => {
													return Err(format_err!(
														"Failed to solve RSA \
														 challenge"
													)
													.into());
												}
											};
											let mut yi = [0; 64];
											y.write_digits(&mut yi, Order::Msf);
											yi
										};

										#[cfg(not(feature = "rug"))]
										let y = {
											let xi = BigUint::from_bytes_be(&x);
											let ni = BigUint::from_bytes_be(&n);
											let mut e = BigUint::one();
											e <<= level as usize;
											let yi = xi.modpow(&e, &ni);
											info!(logger, "Solve RSA puzzle";
											  	  "level" => level, "x" => %xi, "n" => %ni,
											  	  "y" => %yi);
											algs::biguint_to_array(&yi)
										};

										time_reporter.finish();
										info!(logger, "Solve RSA puzzle";
											  "level" => level);
										Ok((x, n, y))
									})
								})
								.map_err(|e| {
									format_err!(
										"Failed to start blocking operation \
										 ({:?})",
										e
									)
									.into()
								})
								.and_then(
									move |r| -> Box<
										Future<Item = _, Error = _> + Send,
									> {
										let (x, n, y) = match r {
											Ok(r) => r,
											Err(e) => {
												return Box::new(future::err(e));
											}
										};
										// Create the command string
										// omega is an ASN.1-DER encoded public key from
										// the ECDH parameters.
										let omega_s = private_key
											.to_pub()
											.to_ts()
											.unwrap();
										let mut command =
											Command::new("clientinitiv");
										command.push("alpha", alpha_s);
										command.push("omega", omega_s);
										command.push("ot", "1");
										// Set ip always except if it is a local address
										if crate::utils::is_global_ip(&ip) {
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

										let packet = Packet::new(
											cheader,
											packets::Data::C2SInit(data),
										);
										Box::new(
											con_value
												.as_packet_sink()
												.send(packet)
												.map(|_| ()),
										)
									},
								)
							})
							.map(|_| ())
							.map_err(move |error| {
								error!(logger2, "Cannot send packet";
									"error" => ?error)
							});
							tokio::spawn(fut);

							let state =
								ServerConnectionState::ClientInitIv { alpha };
							Some((state, None))
						}
					} else {
						None
					}
				})
			}
			ServerConnectionState::Disconnected => {
				warn!(logger, "Got packet from server after disconnecting");
				*is_end = true;
				None
			}
			_ => {
				*ignore_packet = false;
				None
			}
		};
		Ok(res)
	}

	fn handle_command(
		(state, con): &mut (ServerConnectionData, Connection),
		command: &InCommand,
		ignore_packet: &mut bool,
		is_end: &mut bool,
		private_key: EccKeyPrivP256,
		logger: &Logger,
	) -> Result<Option<(ServerConnectionState, Option<Packet>)>>
	{
		let res = match state.state {
			ServerConnectionState::ClientInitIv { ref alpha } => {
				let resender = &mut con.resender;
				let res =
					(|con_params: &mut Option<ConnectedParams>| -> Result<_> {
						let cmd = command.iter().next().unwrap();
						if command.name() == "initivexpand"
							&& cmd.has("alpha")
							&& cmd.has("beta")
							&& cmd.has("omega")
							&& base64::decode(cmd.0["alpha"])
								.map(|a| a == alpha)
								.unwrap_or(false)
						{
							resender.ack_packet(PacketType::Init, 4);

							let beta_vec = base64::decode(cmd.0["beta"])?;
							if beta_vec.len() != 10 {
								return Err(format_err!(
									"Incorrect beta length"
								))?;
							}

							let mut beta = [0; 10];
							beta.copy_from_slice(&beta_vec);
							let server_key =
								EccKeyPubP256::from_ts(cmd.0["omega"])?;

							let (iv, mac) = algs::compute_iv_mac(
								alpha,
								&beta,
								private_key.clone(),
								server_key.clone(),
							)?;
							let params = ConnectedParams::new(
								server_key,
								SharedIv::ProtocolOrig(iv),
								mac,
							);
							*con_params = Some(params);
							Ok(None)
						} else if command.name() == "initivexpand2"
							&& cmd.has("l") && cmd.has("beta")
							&& cmd.has("omega")
							&& cmd.get("ot") == Some("1")
							&& cmd.has("time")
							&& cmd.has("beta")
						{
							resender.ack_packet(PacketType::Init, 4);

							let server_key =
								EccKeyPubP256::from_ts(cmd.0["omega"])?;
							let l = base64::decode(cmd.0["l"])?;
							let proof = base64::decode(cmd.0["proof"])?;
							// Check signature of l (proof)
							server_key.clone().verify(&l, &proof)?;

							let beta_vec = base64::decode(cmd.0["beta"])?;
							if beta_vec.len() != 54 {
								return Err(format_err!(
									"Incorrect beta length"
								)
								.into());
							}

							let mut beta = [0; 54];
							beta.copy_from_slice(&beta_vec);

							// Parse license argument
							let licenses = Licenses::parse(&l)?;
							// Ephemeral key of server
							let server_ek = licenses.derive_public_key()?;

							// Create own ephemeral key
							let ek = EccKeyPrivEd25519::create()?;

							let (iv, mac) = algs::compute_iv_mac31(
								alpha, &beta, &ek, &server_ek,
							)?;
							let params = ConnectedParams::new(
								server_key,
								SharedIv::Protocol31(iv),
								mac,
							);
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

							Ok(Some(Packet::new(
								Header::new(PacketType::Command),
								packets::Data::Command(command),
							)))
						} else {
							Err(format_err!(
								"initivexpand command has wrong arguments"
							)
							.into())
						}
					})(&mut con.params);

				match res {
					Ok(p) => Some((ServerConnectionState::Connecting, p)),
					Err(error) => {
						error!(logger, "Handle udp init packet"; "error" => %error);
						None
					}
				}
			}
			ServerConnectionState::Connecting => {
				let cmd = command.iter().next().unwrap();
				if command.name() == "initserver" {
					// Handle an initserver
					if let Some(params) = &mut con.params {
						if let Ok(c_id) = cmd.get_parse("aclid") {
							params.c_id = c_id;
						}

						let clientinit_id =
							if let SharedIv::Protocol31(_) = params.shared_iv {
								2
							} else {
								1
							};
						// initserver is the ack for clientinit
						// Remove from send queue
						con.resender
							.ack_packet(PacketType::Command, clientinit_id);
					}

					// Notify the resender that we are connected
					con.resender.handle_event(ResenderEvent::Connected);
					*ignore_packet = false;
					Some((ServerConnectionState::Connected, None))
				} else {
					None
				}
			}
			ServerConnectionState::Connected => {
				*ignore_packet = false;
				let cmd = command.iter().next().unwrap();
				if command.name() == "notifyclientleftview" {
					// Handle a disconnect
					if let Some(ref mut params) = con.params {
						if cmd.get_parse("clid") == Ok(params.c_id) {
							*is_end = true;
							// Possible improvement: Wait with the
							// disconnect until we sent the ack.
							Some((ServerConnectionState::Disconnected, None))
						} else {
							None
						}
					} else {
						None
					}
				} else if command.name() == "notifyplugincmd"
					&& cmd.get("name") == Some("cliententerview")
					&& cmd.get("data") == Some("version")
				{
					let mut command = Command::new("plugincmd");
					command.push("name", "cliententerview");
					command.push(
						"data",
						format!(
							"{},{}-{}",
							con.params.as_ref().unwrap().c_id,
							env!("CARGO_PKG_NAME"),
							env!("CARGO_PKG_VERSION"),
						),
					);
					command.push("targetmode", "2");
					// TODO Send to clientid
					//command.push("target", 0);

					let header = Header::new(PacketType::Command);

					let p = Some(Packet::new(
						header,
						packets::Data::Command(command),
					));
					Some((ServerConnectionState::Connected, p))
				} else {
					None
				}
			}
			ServerConnectionState::Disconnected => {
				warn!(logger, "Got packet from server after disconnecting");
				*is_end = true;
				None
			}
			_ => {
				*ignore_packet = false;
				None
			}
		};
		Ok(res)
	}
}
