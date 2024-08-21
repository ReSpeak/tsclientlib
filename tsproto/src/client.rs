use std::net::SocketAddr;
use std::ops::{Deref, DerefMut};
use std::pin::Pin;
use std::str;
use std::task::{Context, Poll};

use base64::prelude::*;
use futures::prelude::*;
#[cfg(not(feature = "rug"))]
use num_bigint::BigUint;
#[cfg(not(feature = "rug"))]
use num_traits::One;
use rand::Rng;
#[cfg(feature = "rug")]
use rug::integer::Order;
#[cfg(feature = "rug")]
use rug::Integer;
use thiserror::Error;
use time::OffsetDateTime;
use tracing::{info, warn};
use tsproto_packets::commands::{CommandItem, CommandParser};
use tsproto_packets::packets::*;
use tsproto_types::crypto::{EccKeyPrivEd25519, EccKeyPrivP256, EccKeyPubEd25519, EccKeyPubP256};

use crate::algorithms as algs;
use crate::connection::{ConnectedParams, Connection, Socket, StreamItem};
use crate::license::Licenses;
use crate::resend::{PacketId, ResenderState};

type Result<T> = std::result::Result<T, Error>;

#[derive(Error, Debug)]
#[non_exhaustive]
pub enum Error {
	#[error("Connection ended unexpectedly")]
	ConnectionEnd,
	#[error("Got initserver, but we have not yet a full connection")]
	EarlyInitserver,
	#[error("Cannot parse base64 argument {0}: {1}")]
	InvalidBase64Arg(&'static str, #[source] base64::DecodeError),
	#[error("Invalid packet: Incorrect beta length in initivexpand2 {0} != 54")]
	InvalidBetaLength(usize),
	#[error("Got invalid client id {0:?}: {1}")]
	InvalidClientId(Vec<u8>, #[source] tsproto_packets::Error),
	#[error("Invalid packet: initivexpand2 command has wrong arguments")]
	InvalidInitivexpand2,
	#[error("Invalid packet: Cannot parse omega as key: {0}")]
	InvalidOmegaKey(#[source] tsproto_types::crypto::Error),
	#[error("Invalid packet: Cannot parse omega as string: {0}")]
	InvalidOmegaString(#[source] tsproto_packets::Error),
	#[error("Invalid packet: {0}")]
	InvalidPacket(&'static str),
	#[error("Invalid packet: Got root for initivexpand2, but length {0} != 32")]
	InvalidRootKeyLength(usize),
	#[error("The server license signature is invalid: {0}")]
	InvalidSignature(#[source] tsproto_types::crypto::Error),
	#[error("Invalid packet: Got multiple {0} in one packet")]
	MultipleCommands(&'static str),
	#[error("Got initserver without accepted client id")]
	NoClientId,
	#[error("Got no ot=1, the server is probably outdated")]
	OutdatedServer,
	#[error("Failed to parse license: {0}")]
	ParseLicense(#[source] crate::license::Error),
	#[error("Failed to solve RSA puzzle")]
	RsaPuzzle,
	#[error("Requested RSA puzzle level {0} is too high")]
	RsaPuzzleTooHighLevel(u32),
	#[error(transparent)]
	TsProto(#[from] crate::Error),
	#[error("Expected {0} but got {1}")]
	UnexpectedPacket(&'static str, String),
}

pub struct Client {
	con: Connection,
	pub private_key: EccKeyPrivP256,
}

impl Client {
	pub fn new(
		address: SocketAddr, udp_socket: Box<dyn Socket + Send>, private_key: EccKeyPrivP256,
	) -> Self {
		Self { con: Connection::new(true, address, udp_socket), private_key }
	}

	async fn get_init(&mut self, init_steps: &[u8]) -> Result<InS2CInitBuf> {
		self.filter_items(|con, i| {
			Ok(match i {
				StreamItem::S2CInit(packet) => {
					if init_steps.contains(&packet.data().data().get_step()) {
						Some(packet)
					} else {
						// Resent packet
						warn!(parent: &con.span, "Got wrong init packet");
						con.hand_back_buffer(packet.into_buffer());
						None
					}
				}
				StreamItem::C2SInit(packet) => {
					warn!(parent: &con.span, "Got init packet from the wrong direction");
					con.hand_back_buffer(packet.into_buffer());
					None
				}
				StreamItem::Error(error) => {
					warn!(parent: &con.span, %error, "Got connection error");
					None
				}
				StreamItem::AckPacket(_) => None,
				StreamItem::NetworkStatsUpdated => None,
				i => {
					warn!(parent: &con.span, got = ?i, "Unexpected packet, wanted S2CInit");
					None
				}
			})
		})
		.await
	}

	async fn get_command(&mut self) -> Result<InCommandBuf> {
		self.filter_items(|con, i| {
			Ok(match i {
				StreamItem::S2CInit(packet) => {
					con.hand_back_buffer(packet.into_buffer());
					None
				}
				StreamItem::C2SInit(packet) => {
					con.hand_back_buffer(packet.into_buffer());
					None
				}
				StreamItem::Command(packet) => Some(packet),
				StreamItem::Error(error) => {
					warn!(parent: &con.span, %error, "Got connection error");
					None
				}
				StreamItem::AckPacket(_) => None,
				StreamItem::NetworkStatsUpdated => None,
				i => {
					warn!(parent: &con.span, got = ?i, "Unexpected packet, wanted Command");
					None
				}
			})
		})
		.await
	}

	/// Drop all packets until the given packet is acknowledged.
	pub async fn wait_for_ack(&mut self, id: PacketId) -> Result<()> {
		self.filter_items(|con, i| {
			Ok(match i {
				StreamItem::S2CInit(packet) => {
					con.hand_back_buffer(packet.into_buffer());
					None
				}
				StreamItem::C2SInit(packet) => {
					con.hand_back_buffer(packet.into_buffer());
					None
				}
				StreamItem::Command(packet) => {
					con.hand_back_buffer(packet.into_buffer());
					None
				}
				StreamItem::Error(error) => {
					warn!(parent: &con.span, %error, "Got connection error");
					None
				}
				StreamItem::AckPacket(ack) => {
					if id <= ack {
						Some(())
					} else {
						None
					}
				}
				StreamItem::NetworkStatsUpdated => None,
				i => {
					warn!(parent: &con.span, got = ?i, "Unexpected packet, wanted Ack");
					None
				}
			})
		})
		.await
	}

	/// Drop all packets until the send queue is not full anymore.
	pub async fn wait_until_can_send(&mut self) -> Result<()> {
		if !self.is_send_queue_full() {
			return Ok(());
		}
		self.filter_items(|con, i| {
			Ok(match i {
				StreamItem::S2CInit(packet) => {
					con.hand_back_buffer(packet.into_buffer());
					None
				}
				StreamItem::C2SInit(packet) => {
					con.hand_back_buffer(packet.into_buffer());
					None
				}
				StreamItem::Command(packet) => {
					con.hand_back_buffer(packet.into_buffer());
					None
				}
				StreamItem::Error(error) => {
					warn!(parent: &con.span, %error, "Got connection error");
					None
				}
				StreamItem::AckPacket(_) => {
					if !con.is_send_queue_full() {
						Some(())
					} else {
						None
					}
				}
				StreamItem::NetworkStatsUpdated => None,
				i => {
					warn!(parent: &con.span, got = ?i, "Unexpected packet, wanted Ack");
					None
				}
			})
		})
		.await
	}

	/// Send a packet. The `send_packet` function will resolve to a future when
	/// the packet has been sent.
	///
	/// If the packet has an acknowledgement (ack or pong), the returned future
	/// will resolve when it is received. Otherwise it will resolve instantly.
	pub fn send_packet(&mut self, packet: OutPacket) -> Result<PacketId> {
		let _span = self.con.span.clone().entered();
		if packet.header().packet_type() == PacketType::Command
			&& packet.content().starts_with(b"clientdisconnect")
		{
			let this = &mut **self;
			this.resender.set_state(ResenderState::Disconnecting);
		}
		Ok(self.con.send_packet(packet)?)
	}

	pub async fn connect(&mut self) -> Result<()> {
		// Send the first init packet
		// Get the current timestamp
		let now = OffsetDateTime::now_utc();
		let timestamp = now.unix_timestamp() as u32;
		// TeamSpeak offsets the timestamp for the version by some years
		let version = timestamp - 1356998400;

		// Random bytes
		let random0 = rand::thread_rng().gen::<[u8; 4]>();

		let alpha;
		loop {
			// Wait for Init1
			{
				self.send_packet(OutC2SInit0::new(version, timestamp, random0))?;
				let init1 = self.get_init(&[1]).await?;
				match init1.data().data() {
					S2CInitData::Init1 { random1, random0_r } => {
						// Check the response
						// Most of the time, random0_r is the reversed random0, but
						// sometimes it isn't so do not check it.
						// random0.as_ref().iter().rev().eq(random0_r.as_ref())
						// The packet is correct.

						// Send next init packet
						self.send_packet(OutC2SInit2::new(version, random1, **random0_r))?;
					}
					_ => {
						return Err(Error::UnexpectedPacket(
							"Init1",
							format!("{:?}", init1.data().packet().header().packet_type()),
						));
					}
				}
			}

			// Wait for Init3
			{
				let init3 = self.get_init(&[3, 127]).await?;
				if init3.data().data().get_step() == 127 {
					continue;
				}
				match init3.data().data() {
					S2CInitData::Init3 { x, n, level, random2 } => {
						let level = *level;
						// Solve RSA puzzle: y = x ^ (2 ^ level) % n
						// Use Montgomery Reduction
						if level > 10_000_000 {
							// Reject too high exponents
							return Err(Error::RsaPuzzleTooHighLevel(level));
						}

						// Create clientinitiv
						alpha = rand::thread_rng().gen::<[u8; 10]>();
						// omega is an ASN.1-DER encoded public key from
						// the ECDH parameters.

						let ip = self.con.address.ip();

						let x = **x;
						let n = **n;
						let random2 = **random2;

						// Use gmp for faster computations if it is
						// available.
						#[cfg(feature = "rug")]
						let y = {
							let mut e = Integer::new();
							let n = Integer::from_digits(&n[..], Order::Msf);
							let x = Integer::from_digits(&x[..], Order::Msf);
							e.set_bit(level, true);
							let y = x.pow_mod(&e, &n).map_err(|_| Error::RsaPuzzle)?;
							let mut yi = [0; 64];
							y.write_digits(&mut yi, Order::Msf);
							yi
						};

						#[cfg(not(feature = "rug"))]
						let y = {
							let xi = BigUint::from_bytes_be(&x);
							let ni = BigUint::from_bytes_be(&n);
							let e = BigUint::one() << level as usize;
							let yi = xi.modpow(&e, &ni);
							info!(parent: &self.con.span, level, x = %xi, n = %ni, y = %yi,
								"Solve RSA puzzle");
							algs::biguint_to_array(&yi)
						};

						// Create the command string
						// omega is an ASN.1-DER encoded public key from
						// the ECDH parameters.
						let omega = self.private_key.to_pub().to_tomcrypt();
						let ip = if crate::utils::is_global_ip(&ip) {
							ip.to_string()
						} else {
							String::new()
						};

						// Send next init packet
						self.send_packet(OutC2SInit4::new(
							version, &x, &n, level, &random2, &y, &alpha, &omega, &ip,
						))?;
					}
					_ => {
						return Err(Error::UnexpectedPacket(
							"Init3",
							format!("{:?}", init3.data().packet().header().packet_type()),
						));
					}
				}
			}
			break;
		}

		let clientek_id;
		{
			let command = self.get_command().await?;

			let (name, args) = CommandParser::new(command.data().packet().content());
			if name != b"initivexpand2" {
				return Err(Error::UnexpectedPacket(
					"initivexpand2",
					format!("{:?}", str::from_utf8(name)),
				));
			}

			let mut l = None;
			let mut beta_vec = None;
			let mut server_key = None;
			let mut proof = None;
			let mut ot = false;
			let mut root = None;
			for item in args {
				match item {
					CommandItem::NextCommand => {
						return Err(Error::MultipleCommands("initivexpand2"));
					}
					CommandItem::Argument(arg) => match arg.name() {
						b"l" => {
							l = Some(
								BASE64_STANDARD
									.decode(arg.value().get())
									.map_err(|e| Error::InvalidBase64Arg("proof", e))?,
							)
						}
						b"beta" => {
							beta_vec = Some(
								BASE64_STANDARD
									.decode(arg.value().get())
									.map_err(|e| Error::InvalidBase64Arg("proof", e))?,
							)
						}
						b"omega" => {
							server_key = Some(
								EccKeyPubP256::from_ts(
									&arg.value().get_str().map_err(Error::InvalidOmegaString)?,
								)
								.map_err(Error::InvalidOmegaKey)?,
							)
						}
						b"proof" => {
							proof = Some(
								BASE64_STANDARD
									.decode(arg.value().get())
									.map_err(|e| Error::InvalidBase64Arg("proof", e))?,
							)
						}
						b"ot" => ot = arg.value().get_raw() == b"1",
						b"root" => {
							let data = BASE64_STANDARD
								.decode(arg.value().get())
								.map_err(|e| Error::InvalidBase64Arg("root", e))?;
							let mut data2 = [0; 32];
							if data.len() != 32 {
								return Err(Error::InvalidRootKeyLength(data.len()));
							}
							data2.copy_from_slice(&data);
							root = Some(EccKeyPubEd25519::from_bytes(data2));
						}
						_ => {}
					},
				}
			}

			if !ot {
				return Err(Error::OutdatedServer);
			}
			if l.is_none() || beta_vec.is_none() || server_key.is_none() || proof.is_none() {
				return Err(Error::InvalidInitivexpand2);
			}
			let l = l.unwrap();
			let beta_vec = beta_vec.unwrap();
			let server_key = server_key.unwrap();
			let proof = proof.unwrap();
			let root = root.unwrap_or_else(|| EccKeyPubEd25519::from_bytes(crate::ROOT_KEY));

			// Check signature of l (proof)
			server_key.verify(&l, &proof).map_err(Error::InvalidSignature)?;

			if beta_vec.len() != 54 {
				return Err(Error::InvalidBetaLength(beta_vec.len()));
			}

			let mut beta = [0; 54];
			beta.copy_from_slice(&beta_vec);

			// Parse license argument
			let licenses = Licenses::parse(l).map_err(Error::ParseLicense)?;
			// Ephemeral key of server
			let server_ek = licenses.derive_public_key(root).map_err(Error::ParseLicense)?;

			// Create own ephemeral key
			let ek = EccKeyPrivEd25519::create();

			let (iv, mac) = algs::compute_iv_mac(&alpha, &beta, &ek, &server_ek);
			self.con.params = Some(ConnectedParams::new(server_key, iv, mac));

			// Send clientek
			let ek_pub = ek.to_pub();
			let ek_s = BASE64_STANDARD.encode(ek_pub.0.as_bytes());

			// Proof: ECDSA signature of ek || beta
			let mut all = Vec::with_capacity(32 + 54);
			all.extend_from_slice(ek_pub.0.as_bytes());
			all.extend_from_slice(&beta);
			let proof = self.private_key.clone().sign(&all);
			let proof_s = BASE64_STANDARD.encode(&proof);

			// Send clientek
			let mut cmd =
				OutCommand::new(Direction::C2S, Flags::empty(), PacketType::Command, "clientek");
			cmd.write_arg("ek", &ek_s);
			cmd.write_arg("proof", &proof_s);
			clientek_id = self.send_packet(cmd.into_packet())?;
		}
		self.wait_for_ack(clientek_id).await?;

		Ok(())
	}

	/// Filter the incoming items.
	pub async fn filter_items<T, F: Fn(&mut Client, StreamItem) -> Result<Option<T>>>(
		&mut self, filter: F,
	) -> Result<T> {
		loop {
			let item = self.next().await;
			match item {
				None => return Err(Error::ConnectionEnd),
				Some(r) => {
					if let Some(r) = filter(self, r?)? {
						return Ok(r);
					}
				}
			}
		}
	}

	/// Filter the incoming items. Drops audio packets.
	pub async fn filter_commands<T, F: Fn(&mut Client, InCommandBuf) -> Result<Option<T>>>(
		&mut self, filter: F,
	) -> Result<T> {
		loop {
			let item = self.next().await;
			match item {
				None => return Err(Error::ConnectionEnd),
				Some(Err(e)) => return Err(e),
				Some(Ok(StreamItem::Error(error))) => {
					warn!(%error, "Got connection error");
				}
				Some(Ok(StreamItem::AckPacket(_))) => {}
				Some(Ok(StreamItem::S2CInit(packet))) => {
					self.hand_back_buffer(packet.into_buffer());
				}
				Some(Ok(StreamItem::C2SInit(packet))) => {
					self.hand_back_buffer(packet.into_buffer());
				}
				Some(Ok(StreamItem::Audio(packet))) => {
					self.hand_back_buffer(packet.into_buffer());
				}
				Some(Ok(StreamItem::Command(packet))) => {
					if let Some(r) = filter(self, packet)? {
						return Ok(r);
					}
				}
				Some(Ok(StreamItem::NetworkStatsUpdated)) => {}
			}
		}
	}

	pub async fn wait_disconnect(&mut self) -> Result<()> {
		loop {
			let item = self.next().await;
			match item {
				None => return Ok(()),
				Some(Err(e)) => return Err(e),
				Some(Ok(StreamItem::AckPacket(_))) => {}
				Some(Ok(StreamItem::Error(error))) => {
					warn!(%error, "Got connection error");
				}
				Some(Ok(StreamItem::S2CInit(packet))) => {
					self.hand_back_buffer(packet.into_buffer());
				}
				Some(Ok(StreamItem::C2SInit(packet))) => {
					self.hand_back_buffer(packet.into_buffer());
				}
				Some(Ok(StreamItem::Audio(packet))) => {
					self.hand_back_buffer(packet.into_buffer());
				}
				Some(Ok(StreamItem::Command(packet))) => {
					self.hand_back_buffer(packet.into_buffer());
				}
				Some(Ok(StreamItem::NetworkStatsUpdated)) => {}
			}
		}
	}

	fn handle_command(&mut self, command: InCommandBuf) -> Result<Option<InCommandBuf>> {
		let _span = self.con.span.clone().entered();
		let (name, args) = CommandParser::new(command.data().packet().content());
		if name == b"initserver" {
			// Handle an initserver
			if let Some(params) = &mut self.params {
				let mut c_id = None;
				for item in args {
					match item {
						CommandItem::NextCommand => {
							return Err(Error::MultipleCommands("initserver"));
						}
						CommandItem::Argument(arg) => {
							if arg.name() == b"aclid" {
								c_id = Some(
									arg.value()
										.get_parse::<tsproto_packets::Error, u16>()
										.map_err(|e| {
											Error::InvalidClientId(
												arg.value().get_raw().to_vec(),
												e,
											)
										})?,
								);
								break;
							}
						}
					}
				}

				if let Some(c_id) = c_id {
					params.c_id = c_id;
				} else {
					return Err(Error::NoClientId);
				}
			} else {
				return Err(Error::EarlyInitserver);
			}

			// Notify the resender that we are connected
			let this = &mut **self;
			this.resender.set_state(ResenderState::Connected);
		} else if name == b"notifyclientleftview" {
			// Handle disconnect
			if let Some(params) = &mut self.params {
				let mut own_client = false;
				for item in args {
					match item {
						CommandItem::NextCommand => {}
						CommandItem::Argument(arg) => {
							if arg.name() == b"clid" {
								let c_id = arg
									.value()
									.get_parse::<tsproto_packets::Error, u16>()
									.map_err(|e| {
									Error::InvalidClientId(arg.value().get_raw().to_vec(), e)
								})?;
								own_client |= c_id == params.c_id;
							}
						}
					}
				}

				if own_client {
					// We are disconnected
					let this = &mut **self;
					this.resender.set_state(ResenderState::Disconnected);
				}
			}
		} else if name == b"notifyplugincmd" {
			let mut is_getversion = false;
			let mut is_getversion_request = false;
			let mut sender: Option<u16> = None;
			for item in args.chain(std::iter::once(CommandItem::NextCommand)) {
				match item {
					CommandItem::Argument(arg) => match arg.name() {
						b"name" => is_getversion = arg.value().get_raw() == b"getversion",
						b"data" => is_getversion_request = arg.value().get_raw() == b"request",
						b"invokerid" => {
							if let Ok(id) = arg.value().get_parse::<tsproto_packets::Error, _>() {
								sender = Some(id);
							}
						}
						_ => {}
					},
					CommandItem::NextCommand => {
						if let Some(sender) = sender {
							if is_getversion && is_getversion_request {
								let mut version = format!(
									"{} {}",
									env!("CARGO_PKG_NAME"),
									git_testament::render_testament!(crate::TESTAMENT),
								);
								#[cfg(debug_assertions)]
								version.push_str(" (Debug)");
								#[cfg(not(debug_assertions))]
								version.push_str(" (Release)");

								let mut cmd = OutCommand::new(
									Direction::C2S,
									Flags::empty(),
									PacketType::Command,
									"plugincmd",
								);
								cmd.write_arg("name", &"getversion");
								cmd.write_arg("data", &version);
								cmd.write_arg("targetmode", &2);
								cmd.write_arg("target", &sender);
								self.send_packet(cmd.into_packet())?;
							}
						}
					}
				}
			}
		}

		Ok(Some(command))
	}
}

impl Deref for Client {
	type Target = Connection;
	fn deref(&self) -> &Self::Target { &self.con }
}

impl DerefMut for Client {
	fn deref_mut(&mut self) -> &mut Self::Target { &mut self.con }
}

/// Return queued errors and inspect packets.
impl Stream for Client {
	type Item = Result<StreamItem>;
	fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
		loop {
			match Pin::new(&mut self.con).poll_next(cx) {
				Poll::Ready(Some(Ok(StreamItem::Command(command)))) => {
					match self.handle_command(command) {
						Err(e) => return Poll::Ready(Some(Err(e))),
						Ok(Some(cmd)) => {
							return Poll::Ready(Some(Ok(StreamItem::Command(cmd))));
						}
						Ok(None) => {}
					}
				}
				Poll::Ready(Some(Err(e))) => return Poll::Ready(Some(Err(e.into()))),
				Poll::Ready(Some(Ok(r))) => return Poll::Ready(Some(Ok(r))),
				Poll::Ready(None) => return Poll::Ready(None),
				Poll::Pending => return Poll::Pending,
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use std::cell::Cell;
	use std::collections::VecDeque;
	use std::io;
	use std::sync::{Arc, Mutex};
	use std::task::Waker;

	use anyhow::{bail, Result};
	use num_traits::ToPrimitive;
	use once_cell::sync::Lazy;
	use tokio::io::ReadBuf;
	use tokio::sync::oneshot;
	use tokio::time::{self, Duration};

	use super::*;
	use crate::connection::Event;
	use crate::resend::PartialPacketId;

	static TRACING: Lazy<()> = Lazy::new(|| tracing_subscriber::fmt().with_test_writer().init());

	#[derive(Clone, Debug)]
	struct SimulatedSocketState {
		buffer: [VecDeque<Vec<u8>>; 2],
		wakers: [Option<Waker>; 2],
	}

	/// Simulate a connection
	#[derive(Clone, Debug)]
	struct SimulatedSocket {
		state: Arc<Mutex<SimulatedSocketState>>,
		/// Index into the wakers list.
		i: usize,
		addr: SocketAddr,
	}

	#[derive(Clone, Debug)]
	struct SimulatedMixSocketState {
		/// The order in which packets are sent to the inner socket.
		///
		/// The default (if the list is empty) is `0, 1, 2, 3, 4, ...`.
		/// To send the third packet first use `2, 0, 1, 3, 4, ...`.
		order: Vec<usize>,
		/// Queued packets that are waiting to be sent to the inner socket
		send_buffer: Vec<Vec<u8>>,
	}

	/// Simulate a connection but mix the packet order
	#[derive(Debug)]
	struct SimulatedMixSocket {
		inner: SimulatedSocket,
		state: Mutex<SimulatedMixSocketState>,
	}

	impl SimulatedSocketState {
		fn new() -> Self { Self { buffer: Default::default(), wakers: Default::default() } }
	}

	impl SimulatedSocket {
		fn new(state: Arc<Mutex<SimulatedSocketState>>, i: usize, addr: SocketAddr) -> Self {
			Self { state, i, addr }
		}

		/// Create a pair of simulated sockets which are connected with each
		/// other.
		fn pair(addr0: SocketAddr, addr1: SocketAddr) -> (SimulatedSocket, SimulatedSocket) {
			let state = Arc::new(Mutex::new(SimulatedSocketState::new()));
			// Switch addresses as we use them as address from receiving packets
			(Self::new(state.clone(), 0, addr1), Self::new(state, 1, addr0))
		}
	}

	impl Socket for SimulatedSocket {
		fn poll_recv_from(
			&self, cx: &mut Context, buf: &mut ReadBuf,
		) -> Poll<io::Result<SocketAddr>> {
			let mut state = self.state.lock().unwrap();
			if let Some(packet) = state.buffer[self.i].pop_front() {
				let len = std::cmp::min(buf.remaining(), packet.len());
				buf.put_slice(&packet[..len]);
				info!(packet = ?&packet[..len], "{} receives packet", self.i);
				Poll::Ready(Ok(self.addr))
			} else {
				state.wakers[self.i] = Some(cx.waker().clone());
				Poll::Pending
			}
		}

		fn poll_send_to(
			&self, _: &mut Context, buf: &[u8], _: SocketAddr,
		) -> Poll<io::Result<usize>> {
			let mut state = self.state.lock().unwrap();
			info!(packet = ?buf, "{} sends packet", self.i);
			state.buffer[1 - self.i].push_back(buf.to_vec());
			if let Some(waker) = state.wakers[1 - self.i].take() {
				waker.wake();
			}
			Poll::Ready(Ok(buf.len()))
		}

		fn local_addr(&self) -> io::Result<SocketAddr> { Ok(self.addr) }
	}

	impl SimulatedMixSocket {
		fn new(inner: SimulatedSocket, order: Vec<usize>) -> Self {
			let mut check_order = order.clone();
			check_order.sort();
			assert!(check_order.is_empty() || check_order.windows(2).all(|win| win[0] != win[1]));
			Self {
				inner,
				state: Mutex::new(SimulatedMixSocketState {
					order,
					send_buffer: Default::default(),
				}),
			}
		}

		fn pair(
			addr0: SocketAddr, addr1: SocketAddr, order0: Vec<usize>, order1: Vec<usize>,
		) -> (SimulatedMixSocket, SimulatedMixSocket) {
			let (inner0, inner1) = SimulatedSocket::pair(addr0, addr1);
			(Self::new(inner0, order0), Self::new(inner1, order1))
		}
	}

	impl Socket for SimulatedMixSocket {
		fn poll_recv_from(
			&self, cx: &mut Context, buf: &mut ReadBuf,
		) -> Poll<io::Result<SocketAddr>> {
			self.inner.poll_recv_from(cx, buf)
		}

		fn poll_send_to(
			&self, cx: &mut Context, buf: &[u8], target: SocketAddr,
		) -> Poll<io::Result<usize>> {
			let mut state = self.state.lock().unwrap();
			state.send_buffer.push(buf.to_vec());

			// Send packets from send_buffer which are ready
			while !state.order.is_empty() {
				let next = state.order[0];
				if state.send_buffer.len() <= next {
					break;
				}
				state.order.remove(0);
				info!("Sending packet {}", next);
				let packet = state.send_buffer.remove(next);
				let send_res = self.inner.poll_send_to(cx, &packet, target);
				let Poll::Ready(Ok(len)) = send_res else {
					panic!("Unexpected send result {:?}", send_res)
				};
				assert_eq!(len, packet.len());

				// Shift all following packet ids
				// After sending packet 2
				// 2, 0, 1, 3, 4
				// becomes
				// 0, 1, 2, 3
				for order in &mut state.order {
					if *order > next {
						*order -= 1;
					}
				}
			}

			if state.order.is_empty() {
				for packet in std::mem::take(&mut state.send_buffer) {
					// Send all packets in order
					info!("Sending packet in order");
					let send_res = self.inner.poll_send_to(cx, &packet, target);
					let Poll::Ready(Ok(len)) = send_res else {
						panic!("Unexpected send result {:?}", send_res)
					};
					assert_eq!(len, packet.len());
				}
			}

			Poll::Ready(Ok(buf.len()))
		}

		fn local_addr(&self) -> io::Result<SocketAddr> { self.inner.local_addr() }
	}

	pub struct TestConnection {
		pub client: Client,
		pub server: Client,
	}

	impl TestConnection {
		pub fn new() -> Result<Self> {
			let addr = "127.0.0.1:0".parse()?;
			let (socket0, socket1) = SimulatedSocket::pair(addr, addr);
			Self::new_with_sockets(Box::new(socket0), Box::new(socket1))
		}

		pub fn new_with_sockets(
			socket0: Box<dyn Socket + Send>, socket1: Box<dyn Socket + Send>,
		) -> Result<Self> {
			Lazy::force(&TRACING);

			let addr = "127.0.0.1:0".parse()?;
			let client_key = EccKeyPrivP256::create();
			let server_key = EccKeyPrivP256::create();

			let mut client = Client::new(addr, socket0, client_key);
			let mut server = Client::new(addr, socket1, server_key);
			server.is_client = false;

			// TODO Span is = "client"
			crate::log::add_logger_with_verbosity(3, &mut client);
			// TODO Span is = "server"
			crate::log::add_logger_with_verbosity(3, &mut server);

			Ok(Self { client, server })
		}

		pub fn set_connected(&mut self) {
			Self::set_con_connected(&mut self.client, self.server.private_key.to_pub());
			Self::set_con_connected(&mut self.server, self.client.private_key.to_pub());
		}

		/// Set the connection to connected.
		fn set_con_connected(con: &mut Client, other_key: EccKeyPubP256) {
			let con = &mut **con;
			con.resender.set_state(ResenderState::Connected);

			con.codec.outgoing_p_ids[PacketType::Command.to_usize().unwrap()] =
				PartialPacketId { generation_id: 0, packet_id: 1 };
			con.codec.incoming_p_ids[PacketType::Command.to_usize().unwrap()] =
				PartialPacketId { generation_id: 0, packet_id: 1 };
			con.codec.outgoing_p_ids[PacketType::Ack.to_usize().unwrap()] =
				PartialPacketId { generation_id: 0, packet_id: 1 };
			con.codec.incoming_p_ids[PacketType::Ack.to_usize().unwrap()] =
				PartialPacketId { generation_id: 0, packet_id: 1 };

			// Set params
			let mut params = ConnectedParams::new(other_key, [0; 64], [0x42; 8]);
			params.c_id = 1;
			con.params = Some(params);
		}
	}

	/// Check if init packet is sent and connect timeout is working.
	#[tokio::test]
	async fn connect_timeout() -> Result<()> {
		let mut state = TestConnection::new()?;

		let (send, recv) = oneshot::channel();
		let send = Cell::new(Some(send));
		let listener = move |event: &Event| {
			if let Event::ReceivePacket(packet) = event {
				if packet.header().packet_type() == PacketType::Init {
					if let Some(s) = send.replace(None) {
						s.send(()).unwrap();
					}
				}
			}
		};

		state.server.event_listeners.push(Box::new(listener));

		tokio::select!(
			(r, err) = future::join(
				time::timeout(Duration::from_secs(10), recv),
				state.client.connect(),
			) => {
				r??;
				assert!(err.is_err(), "Connect should timeout");
			}
			_ = state.server.wait_disconnect() => {
				bail!("Server should just run in the background");
			}
		);
		Ok(())
	}

	#[tokio::test]
	async fn disconnect_timeout() -> Result<()> {
		let mut state = TestConnection::new()?;
		state.set_connected();

		let (send, recv) = oneshot::channel();
		let send = Cell::new(Some(send));
		let listener = move |event: &Event| {
			if let Event::ReceivePacket(packet) = event {
				if packet.header().packet_type() == PacketType::Command {
					send.replace(None).unwrap().send(()).unwrap();
				}
			}
		};

		state.server.event_listeners.push(Box::new(listener));

		let mut cmd = OutCommand::new(
			Direction::C2S,
			Flags::empty(),
			PacketType::Command,
			"clientdisconnect",
		);
		cmd.write_arg("reasonid", &8);
		cmd.write_arg("reasonmsg", &"Bye");

		state.client.send_packet(cmd.into_packet())?;

		tokio::select!(
			(r, err) = future::join(
				time::timeout(Duration::from_secs(10), recv),
				state.client.wait_disconnect(),
			) => {
				r??;
				assert!(err.is_err(), "Disconnect should timeout");
			}
			_ = state.server.wait_disconnect() => {
				bail!("Server should just run in the background");
			}
		);
		Ok(())
	}

	#[tokio::test]
	async fn timeout() -> Result<()> {
		let mut state = TestConnection::new()?;
		state.set_connected();

		let r = state.client.wait_disconnect().await;
		assert!(
			matches!(r, super::Result::<()>::Err(Error::TsProto(crate::Error::Timeout(_)))),
			"Connection should timeout (result: {:?})",
			r
		);
		Ok(())
	}

	/// Send ping and check that a pong is received.
	#[tokio::test]
	async fn pong() -> Result<()> {
		let mut state = TestConnection::new()?;
		state.set_connected();

		let packet = OutPacket::new_with_dir(Direction::S2C, Flags::UNENCRYPTED, PacketType::Ping);
		state.server.send_packet(packet.clone())?;
		state.server.send_packet(packet.clone())?;

		let (send, recv) = oneshot::channel();
		let send = Cell::new(Some(send));
		let counter = Cell::new(0u8);
		let listener = move |event: &Event| {
			if let Event::ReceivePacket(packet) = event {
				if packet.header().packet_type() == PacketType::Pong {
					counter.set(counter.get() + 1);
					// Until 2 pongs are received
					if counter.get() == 2 {
						send.replace(None).unwrap().send(()).unwrap();
					}
				}
			}
		};

		state.server.event_listeners.push(Box::new(listener));

		tokio::select!(
			r = time::timeout(Duration::from_secs(5), recv) => {
				r??;
				return Ok(());
			}
			_ = state.client.wait_disconnect() => {}
			_ = state.server.wait_disconnect() => {}
		);
		bail!("Unexpected disconnect")
	}

	/// Check that the packet id wraps around.
	#[tokio::test]
	async fn generation_id() -> Result<()> {
		let mut state = TestConnection::new()?;
		state.set_connected();

		// Sending 70 000 messages takes about 7 minutes in debug mode so we
		// start at 65 500 and send only 100.
		let count = 100;

		// Set current id
		for c in [&mut state.client, &mut state.server] {
			c.codec.outgoing_p_ids[PacketType::Command.to_usize().unwrap()] =
				PartialPacketId { generation_id: 0, packet_id: 65_500 };
			c.codec.incoming_p_ids[PacketType::Command.to_usize().unwrap()] =
				PartialPacketId { generation_id: 0, packet_id: 65_500 };
			c.codec.outgoing_p_ids[PacketType::Ack.to_usize().unwrap()] =
				PartialPacketId { generation_id: 0, packet_id: 65_500 };
			c.codec.incoming_p_ids[PacketType::Ack.to_usize().unwrap()] =
				PartialPacketId { generation_id: 0, packet_id: 65_500 };
		}

		let (send, recv) = oneshot::channel();
		let send = Cell::new(Some(send));
		let counter = Cell::new(0u8);
		let listener = move |event: &Event| {
			if let Event::ReceivePacket(packet) = event {
				if packet.header().packet_type() == PacketType::Command {
					counter.set(counter.get() + 1);
					if counter.get() == count {
						send.replace(None).unwrap().send(()).unwrap();
					}
				}
			}
		};

		state.client.event_listeners.push(Box::new(listener));

		for i in 0..count {
			let mut cmd = OutCommand::new(
				Direction::S2C,
				Flags::empty(),
				PacketType::Command,
				"notifytextmessage",
			);
			cmd.write_arg("msg", &format!("message {}", i));
			state.server.send_packet(cmd.into_packet())?;
		}

		tokio::select!(
			r = recv => {
				r?;
				return Ok(());
			}
			_ = state.client.wait_disconnect() => {}
			_ = state.server.wait_disconnect() => {}
		);
		bail!("Unexpected disconnect")
	}

	/// Check that out of order packets are decoded in the correct order.
	async fn out_of_order_test(order: Vec<usize>) -> Result<()> {
		let addr = "127.0.0.1:0".parse()?;
		let (socket0, socket1) = SimulatedMixSocket::pair(addr, addr, vec![], order);
		let mut state = TestConnection::new_with_sockets(Box::new(socket0), Box::new(socket1))?;
		state.set_connected();

		let count = 10;

		let (send, recv) = oneshot::channel();
		let send = Cell::new(Some(send));
		let counter = Cell::new(0u8);
		let listener = move |event: &Event| {
			if let Event::ReceivePacket(packet) = event {
				if packet.header().packet_type() == PacketType::Command {
					let Some(content) =
						packet.content().strip_prefix(b"notifytextmessage msg=message\\s")
					else {
						panic!("Expected text message but got '{:?}'", packet.content())
					};
					let i: u8 = std::str::from_utf8(content).unwrap().parse().unwrap();
					let cnt = counter.get();
					assert_eq!(cnt, i, "Messages are not in the correct order");
					counter.set(cnt + 1);
					if counter.get() == count {
						send.replace(None).unwrap().send(()).unwrap();
					}
				}
			}
		};

		state.client.event_listeners.push(Box::new(listener));

		for i in 0..count {
			let mut cmd = OutCommand::new(
				Direction::S2C,
				Flags::empty(),
				PacketType::Command,
				"notifytextmessage",
			);
			cmd.write_arg("msg", &format!("message {}", i));
			state.server.send_packet(cmd.into_packet())?;
		}

		tokio::select!(
			r = recv => {
				r?;
				return Ok(());
			}
			_ = state.client.wait_disconnect() => {}
			_ = state.server.wait_disconnect() => {}
		);
		bail!("Unexpected disconnect")
	}

	// Check that out of order packets are decoded in the correct order.

	#[tokio::test]
	async fn in_order() -> Result<()> { out_of_order_test(vec![]).await }

	#[tokio::test]
	async fn single_out_of_order() -> Result<()> { out_of_order_test(vec![2]).await }

	#[tokio::test]
	async fn reversed_order() -> Result<()> {
		out_of_order_test(vec![9, 8, 7, 6, 5, 4, 3, 2, 1, 0]).await
	}

	#[tokio::test]
	async fn some_out_of_order() -> Result<()> {
		out_of_order_test(vec![5, 3, 7, 8, 2, 0, 6, 1]).await
	}
}
