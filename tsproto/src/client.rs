use std::net::SocketAddr;
use std::ops::{Deref, DerefMut};
use std::pin::Pin;
use std::task::{Context, Poll};

use anyhow::bail;
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
use slog::{info, warn, Level, Logger};
use time::OffsetDateTime;
use tsproto_packets::packets::*;

use crate::algorithms as algs;
use crate::connection::{ConnectedParams, Connection, Socket, StreamItem};
use crate::crypto::{EccKeyPrivEd25519, EccKeyPrivP256, EccKeyPubP256};
use crate::license::Licenses;
use crate::resend::{PacketId, ResenderState};
use crate::Result;

pub struct Client {
	con: Connection,
	pub private_key: EccKeyPrivP256,
}

impl Client {
	pub fn new(
		logger: Logger,
		address: SocketAddr,
		udp_socket: Box<dyn Socket>,
		private_key: EccKeyPrivP256,
	) -> Self
	{
		Self {
			con: Connection::new(true, logger, address, udp_socket),
			private_key,
		}
	}

	async fn get_init(&mut self, init_step: u8) -> Result<InS2CInitBuf> {
		self.filter_items(|con, i| Ok(match i {
			StreamItem::S2CInit(packet) => {
				if packet.data().data().get_step() == init_step {
					Some(packet)
				} else {
					// Resent packet
					warn!(con.logger, "Got wrong init packet");
					con.hand_back_buffer(packet.into_buffer());
					None
				}
			}
			StreamItem::C2SInit(packet) => {
				warn!(con.logger, "Got init packet from the wrong direction");
				con.hand_back_buffer(packet.into_buffer());
				None
			}
			StreamItem::Error(e) => {
				warn!(con.logger, "Got connection error"; "error" => %e);
				None
			}
			StreamItem::AckPacket(_) => None,
			i => {
				warn!(con.logger, "Unexpected packet, wanted S2CInit"; "got" => ?i);
				None
			}
		})).await
	}

	async fn get_command(&mut self) -> Result<InCommandBuf> {
		self.filter_items(|con, i| Ok(match i {
			StreamItem::S2CInit(packet) => {
				con.hand_back_buffer(packet.into_buffer());
				None
			}
			StreamItem::C2SInit(packet) => {
				con.hand_back_buffer(packet.into_buffer());
				None
			}
			StreamItem::Command(packet) => Some(packet),
			StreamItem::Error(e) => {
				warn!(con.logger, "Got connection error"; "error" => %e);
				None
			}
			StreamItem::AckPacket(_) => None,
			i => {
				warn!(con.logger, "Unexpected packet, wanted Command"; "got" => ?i);
				None
			}
		})).await
	}

	/// Drop all packets until the given packet is acknowledged.
	pub async fn wait_for_ack(&mut self, id: PacketId) -> Result<()> {
		self.filter_items(|con, i| Ok(match i {
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
			StreamItem::Error(e) => {
				warn!(con.logger, "Got connection error"; "error" => %e);
				None
			}
			StreamItem::AckPacket(ack) => {
				if id <= ack {
					Some(())
				} else {
					None
				}
			}
			i => {
				warn!(con.logger, "Unexpected packet, wanted Ack"; "got" => ?i);
				None
			}
		})).await
	}

	/// Drop all packets until the send queue is not full anymore.
	pub async fn wait_until_can_send(&mut self) -> Result<()> {
		if !self.is_send_queue_full() {
			return Ok(());
		}
		self.filter_items(|con, i| Ok(match i {
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
			StreamItem::Error(e) => {
				warn!(con.logger, "Got connection error"; "error" => %e);
				None
			}
			StreamItem::AckPacket(_) => {
				if !con.is_send_queue_full() {
					Some(())
				} else {
					None
				}
			}
			i => {
				warn!(con.logger, "Unexpected packet, wanted Ack"; "got" => ?i);
				None
			}
		})).await
	}

	/// Send a packet. The `send_packet` function will resolve to a future when
	/// the packet has been sent.
	///
	/// If the packet has an acknowledgement (ack or pong), the returned future
	/// will resolve when it is received. Otherwise it will resolve instantly.
	pub fn send_packet(&mut self, packet: OutPacket) -> Result<PacketId> {
		if packet.header().packet_type() == PacketType::Command
			&& packet.content().starts_with(b"clientdisconnect") {
			let this = &mut **self;
			this.resender.set_state(&this.logger, ResenderState::Disconnecting);
		}
		self.con.send_packet(packet)
	}

	// TODO Remove return codes?
	/*/// Send a packet. The `send_packet` function will resolve to a future when
	/// the packet has been sent.
	///
	/// A `return_code` will be appended to the command
	/// If the packet has an acknowledgement (ack or pong), the returned future
	/// will resolve when it is received. Otherwise it will resolve instantly.
	///
	/// Returns the TeamSpeak error id.
	pub async fn send_packet_with_answer(&mut self, mut packet: OutPacket) -> impl Future<Output=Result<u32>> {
		assert!(packet.header().packet_type().is_command(),
			"Can only send commands with `send_packet_with_answer`");
		packet.data_mut().extend_from_slice(b" return_code=");
		packet.data_mut().extend_from_slice(self.cur_return_code.to_string().as_bytes());

		let (send, recv) = oneshot::channel();
		self.return_codes.insert(self.cur_return_code, send);
		self.cur_return_code += 1;

		self.send_packet(packet).await.and_then(|_| recv.map_err(|e| e.into()))
	}*/

	pub async fn connect(&mut self) -> Result<()> {
		// Send the first init packet
		// Get the current timestamp
		let now = OffsetDateTime::now();
		let timestamp = now.timestamp() as u32;
		let mut rng = rand::thread_rng();

		// Random bytes
		let random0 = rng.gen::<[u8; 4]>();

		// Wait for Init1
		{
			self.send_packet(OutC2SInit0::new(timestamp, timestamp, random0))?;
			let init1 = self.get_init(1).await?;
			match init1.data().data() {
				S2CInitData::Init1 { random1, random0_r } => {
					// Check the response
					// Most of the time, random0_r is the reversed random0, but
					// sometimes it isn't so do not check it.
					// random0.as_ref().iter().rev().eq(random0_r.as_ref())
					// The packet is correct.

					// Send next init packet
					self.send_packet(OutC2SInit2::new(
						timestamp,
						random1,
						**random0_r,
					))?;
				}
				_ => bail!("Unexpected init packet, needs Init1"),
			}
		}

		// Wait for Init3
		let alpha;
		{
			let init3 = self.get_init(3).await?;
			match init3.data().data() {
				S2CInitData::Init3 { x, n, level, random2 } => {
					let level = *level;
					// Solve RSA puzzle: y = x ^ (2 ^ level) % n
					// Use Montgomery Reduction
					if level > 10_000_000 {
						// Reject too high exponents
						bail!("Requested level {} is too high", level);
					}

					// Create clientinitiv
					let mut rng = rand::thread_rng();
					alpha = rng.gen::<[u8; 10]>();
					// omega is an ASN.1-DER encoded public key from
					// the ECDH parameters.

					let ip = self.con.address.ip();

					let x = **x;
					let n = **n;
					let random2 = **random2;

					let mut time_reporter = slog_perf::TimeReporter::new_with_level(
						"Solve RSA puzzle", self.con.logger.clone(),
						Level::Info);
					time_reporter.start("");

					// Use gmp for faster computations if it is
					// available.
					#[cfg(feature = "rug")]
					let y = {
						let mut e = Integer::new();
						let n = Integer::from_digits(&n[..], Order::Msf);
						let x = Integer::from_digits(&x[..], Order::Msf);
						e.set_bit(level, true);
						let y = x.pow_mod(&e, &n).map_err(|_|
							format_err!("Failed to solve RSA puzzle"))?;
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
						info!(self.con.logger, "Solve RSA puzzle";
							"level" => level, "x" => %xi, "n" => %ni,
							"y" => %yi);
						algs::biguint_to_array(&yi)
					};

					time_reporter.finish();
					info!(self.con.logger, "Solve RSA puzzle"; "level" => level);

					// Create the command string
					// omega is an ASN.1-DER encoded public key from
					// the ECDH parameters.
					let omega = self.private_key.to_pub().to_tomcrypt()?;
					let ip = if crate::utils::is_global_ip(&ip) {
						ip.to_string()
					} else {
						String::new()
					};

					// Send next init packet
					self.send_packet(OutC2SInit4::new(
						timestamp, &x, &n, level, &random2,
						&y, &alpha, &omega, &ip,
					))?;
				}
				_ => bail!("Unexpected init packet, needs Init3"),
			}
		}

		let clientek_id;
		{
			let command = self.get_command().await?;
			let cmd = command.data().iter().next().unwrap();
			if command.data().data().name != "initivexpand2"
				|| !cmd.has("l") || !cmd.has("beta")
				|| !cmd.has("omega") || cmd.get("ot") != Some("1")
				|| !cmd.has("time") || !cmd.has("beta")
			{
				bail!("initivexpand2 command has wrong arguments");
			}

			let server_key = EccKeyPubP256::from_ts(cmd.0["omega"])?;
			let l = base64::decode(cmd.0["l"])?;
			let proof = base64::decode(cmd.0["proof"])?;
			// Check signature of l (proof)
			server_key.clone().verify(&l, &proof)?;

			let beta_vec = base64::decode(cmd.0["beta"])?;
			if beta_vec.len() != 54 {
				bail!("Incorrect beta length: {} != 54", beta_vec.len());
			}

			let mut beta = [0; 54];
			beta.copy_from_slice(&beta_vec);

			// Parse license argument
			let licenses = Licenses::parse(&l)?;
			// Ephemeral key of server
			let server_ek = licenses.derive_public_key()?;

			// Create own ephemeral key
			let ek = EccKeyPrivEd25519::create()?;

			let (iv, mac) = algs::compute_iv_mac(
				&alpha, &beta, &ek, &server_ek,
			)?;
			self.con.params = Some(ConnectedParams::new(server_key, iv, mac));

			// Send clientek
			let ek_pub = ek.to_pub();
			let ek_s = base64::encode(ek_pub.0.as_bytes());

			// Proof: ECDSA signature of ek || beta
			let mut all = Vec::with_capacity(32 + 54);
			all.extend_from_slice(ek_pub.0.as_bytes());
			all.extend_from_slice(&beta);
			let proof = self.private_key.clone().sign(&all)?;
			let proof_s = base64::encode(&proof);

			// Send clientek
			clientek_id = self.send_packet(OutCommand::new::<
				_,
				_,
				String,
				String,
				_,
				_,
				std::iter::Empty<_>,
			>(
				Direction::C2S,
				PacketType::Command,
				"clientek",
				vec![("ek", ek_s), ("proof", proof_s)]
					.into_iter(),
				std::iter::empty(),
			))?;
		}
		self.wait_for_ack(clientek_id).await?;

		Ok(())
	}

	/// Filter the incoming items.
	pub async fn filter_items<T, F: Fn(&mut Client, StreamItem) -> Result<Option<T>>>(&mut self, filter: F) -> Result<T> {
		loop {
			let item = self.next().await;
			match item {
				None => bail!("Connection ended before a matching item was found"),
				Some(r) => {
					if let Some(r) = filter(self, r?)? {
						return Ok(r);
					}
				}
			}
		}
	}

	/// Filter the incoming items. Drops audio packets.
	pub async fn filter_commands<T, F: Fn(&mut Client, InCommandBuf) -> Result<Option<T>>>(&mut self, filter: F) -> Result<T> {
		loop {
			let item = self.next().await;
			match item {
				None => bail!("Connection ended before a matching item was found"),
				Some(Err(e)) => return Err(e),
				Some(Ok(StreamItem::Error(e))) => {
					warn!(self.logger, "Got connection error"; "error" => %e);
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
				Some(Ok(StreamItem::Error(e))) => {
					warn!(self.logger, "Got connection error"; "error" => %e);
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
			}
		}
	}

	fn handle_command(&mut self, command: InCommandBuf) -> Result<Option<InCommandBuf>> {
		let cmd_name = &command.data().data().name;
		if cmd_name == &"initserver" {
			// Handle an initserver
			let cmd = command.data().iter().next().unwrap();
			if let Some(params) = &mut self.params {
				match cmd.get_parse("aclid") {
					Ok(c_id) => params.c_id = c_id,
					Err(e) => bail!("Failed to parse aclid: {:?}", e),
				}
			} else {
				bail!("Got initserver, but we have not yet a full connection");
			}

			// Notify the resender that we are connected
			let this = &mut **self;
			this.resender.set_state(&this.logger, ResenderState::Connected);
		} else if cmd_name == &"notifyclientleftview" {
			// Handle disconnect
			let cmd = command.data().iter().next().unwrap();
			if let Some(params) = &mut self.params {
				if cmd.get_parse("clid") == Ok(params.c_id) {
					// We are disconnected
					let this = &mut **self;
					this.resender.set_state(&this.logger, ResenderState::Disconnected);
				}
			}
		} else if cmd_name == &"notifyplugincmd" {
			let cmd = command.data().iter().next().unwrap();
			// TODO getversion
			if cmd.get("name") == Some("getversion") && cmd.has("client") {
				if let Some(sender) = cmd.get("client") {
					let mut version = format!(
						"{} {}",
						env!("CARGO_PKG_NAME"),
						git_testament::render_testament!(crate::TESTAMENT),
					);
					#[cfg(debug_assertions)]
					version.push_str(" (Debug)");
					#[cfg(not(debug_assertions))]
					version.push_str(" (Release)");

					self.send_packet(OutCommand::new::<
						_,
						_,
						String,
						String,
						_,
						_,
						std::iter::Empty<_>,
					>(
						Direction::C2S,
						PacketType::Command,
						"plugincmd",
						vec![
							("name", "getversion".into()),
							("data", version),
							// PluginTargetMode::Client
							("targetmode", 2.to_string()),
							("target", sender.to_string()),
						]
						.into_iter(),
						std::iter::empty(),
					))?;
				}
			}
			// TODO Remove return codes?
		/*} else if cmd_name == &"error" {
			let cmd = command.data().iter().next().unwrap();
			if let Ok(code) = cmd.get_parse("return_code") {
				if let Some(send) = self.return_codes.remove(&code) {
					if let Ok(id) = cmd.get_parse("error_id") {
						let _ = send.send(id);
						return Ok(None);
					}
				}
			}*/
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
			match Pin::new(&mut **self).poll_next(cx) {
				Poll::Ready(Some(Ok(StreamItem::Command(command)))) => {
					match self.handle_command(command) {
						Err(e) => return Poll::Ready(Some(Err(e))),
						Ok(Some(cmd)) => return Poll::Ready(Some(Ok(
							StreamItem::Command(cmd)))),
						Ok(None) => {}
					}
				}
				r => return r,
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

	use num_traits::ToPrimitive;
	use slog::{debug, o, Drain};
	use tokio::sync::oneshot;
	use tokio::time::{self, Duration};

	use crate::connection::Event;
	use super::*;

	#[derive(Clone, Debug)]
	struct SimulatedSocketState {
		logger: Logger,
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

	impl SimulatedSocketState {
		fn new(logger: Logger) -> Self {
			Self {
				logger,
				buffer: Default::default(),
				wakers: Default::default(),
			}
		}
	}

	impl SimulatedSocket {
		fn new(state: Arc<Mutex<SimulatedSocketState>>, i: usize, addr: SocketAddr) -> Self {
			Self { state, i, addr }
		}

		/// Create a pair of simulated sockets which are connected with each
		/// other.
		fn pair(logger: Logger, addr0: SocketAddr, addr1: SocketAddr) -> (SimulatedSocket, SimulatedSocket) {
			let state = Arc::new(Mutex::new(SimulatedSocketState::new(logger)));
			// Switch addresses as we use them as address from receiving packets
			(Self::new(state.clone(), 0, addr1), Self::new(state, 1, addr0))
		}
	}

	impl Socket for SimulatedSocket {
		fn poll_recv_from(&self, cx: &mut Context, buf: &mut [u8]) -> Poll<io::Result<(usize, SocketAddr)>> {
			let mut state = self.state.lock().unwrap();
			if let Some(packet) = state.buffer[self.i].pop_front() {
				let len = std::cmp::min(buf.len(), packet.len());
				buf[..len].copy_from_slice(&packet[..len]);
				debug!(state.logger, "{} receives packet", self.i);
				Poll::Ready(Ok((len, self.addr)))
			} else {
				state.wakers[self.i] = Some(cx.waker().clone());
				Poll::Pending
			}
		}

		fn poll_send_to(&self, _: &mut Context, buf: &[u8], _: &SocketAddr) -> Poll<io::Result<usize>> {
			let mut state = self.state.lock().unwrap();
			debug!(state.logger, "{} sends packet", self.i);
			state.buffer[1 - self.i].push_back(buf.to_vec());
			if let Some(waker) = state.wakers[1 - self.i].take() {
				waker.wake();
			}
			Poll::Ready(Ok(buf.len()))
		}

		fn local_addr(&self) -> io::Result<SocketAddr> { Ok(self.addr) }
	}

	pub struct TestConnection {
		pub client: Client,
		pub server: Client,
	}

	impl TestConnection {
		pub fn new() -> Result<Self> {
			let logger = {
				let decorator = slog_term::PlainDecorator::new(slog_term::TestStdoutWriter);
				let drain = Mutex::new(slog_term::FullFormat::new(decorator).build()).fuse();

				slog::Logger::root(drain, o!())
			};

			let addr = "127.0.0.1:0".parse()?;
			let client_key = EccKeyPrivP256::create()?;
			let server_key = EccKeyPrivP256::create()?;

			let (socket0, socket1) = SimulatedSocket::pair(logger.clone(), addr, addr);
			let mut client = Client::new(logger.clone(), addr, Box::new(socket0), client_key);
			let mut server = Client::new(logger.clone(), addr, Box::new(socket1), server_key);
			server.is_client = false;

			crate::log::add_logger(logger.new(o!("is" => "client")), 2, &mut client);
			crate::log::add_logger(logger.new(o!("is" => "server")), 2, &mut server);

			Ok(Self { client, server })
		}

		pub async fn set_connected(&mut self) {
			Self::set_con_connected(&mut self.client, self.server.private_key.to_pub()).await;
			Self::set_con_connected(&mut self.server, self.client.private_key.to_pub()).await;
		}

		/// Set the connection to connected.
		async fn set_con_connected(con: &mut Client, other_key: EccKeyPubP256) {
			let con = &mut **con;
			con.resender.set_state(&con.logger, ResenderState::Connected);

			con.codec.outgoing_p_ids[PacketType::Command.to_usize().unwrap()] = (0, 1);
			con.codec.incoming_p_ids[PacketType::Command.to_usize().unwrap()] = (0, 1);
			con.codec.outgoing_p_ids[PacketType::Ack.to_usize().unwrap()] = (0, 1);
			con.codec.incoming_p_ids[PacketType::Ack.to_usize().unwrap()] = (0, 1);

			// Set params
			con.params = Some(ConnectedParams::new(
				other_key,
				[0; 64],
				[0x42; 8],
			));
			let params = con.params.as_mut().unwrap();
			params.c_id = 1;
		}
	}

	/// Check if init packet is sent and connect timeout is working.
	#[tokio::test]
	async fn test_connect_timeout() -> Result<()> {
		let mut state = TestConnection::new()?;

		let (send, recv) = oneshot::channel();
		let send = Cell::new(Some(send));
		let listener = move |event: &Event| match event {
			Event::ReceivePacket(packet) => {
				if packet.header().packet_type() == PacketType::Init {
					if let Some(s) = send.replace(None) {
						s.send(()).unwrap();
					}
				}
			}
			_ => {}
		};

		state.server.event_listeners.push(Box::new(listener));

		tokio::select!(
			(r, err) = future::join(
				time::timeout(Duration::from_secs(10), recv),
				state.client.connect(),
			) => {
				r??;
				assert!(err.is_err(), "Connect should timeout");
				return Ok(());
			}
			_ = state.server.wait_disconnect() => {
				bail!("Server should just run in the background");
			}
		);
	}

	#[tokio::test]
	async fn test_disconnect_timeout() -> Result<()> {
		let mut state = TestConnection::new()?;
		state.set_connected().await;

		let (send, recv) = oneshot::channel();
		let send = Cell::new(Some(send));
		let listener = move |event: &Event| match event {
			Event::ReceivePacket(packet) => {
				if packet.header().packet_type() == PacketType::Command {
					send.replace(None).unwrap().send(()).unwrap();
				}
			}
			_ => {}
		};

		state.server.event_listeners.push(Box::new(listener));

		let packet =
			OutCommand::new::<_, _, String, String, _, _, std::iter::Empty<_>>(
				Direction::C2S,
				PacketType::Command,
				"clientdisconnect",
				vec![
					// Reason: Disconnect
					("reasonid", "8"),
					("reasonmsg", "Bye"),
				]
				.into_iter(),
				std::iter::empty(),
			);
		state.client.send_packet(packet)?;

		tokio::select!(
			(r, err) = future::join(
				time::timeout(Duration::from_secs(10), recv),
				state.client.wait_disconnect(),
			) => {
				r??;
				assert!(err.is_err(), "Connect should timeout");
				return Ok(());
			}
			_ = state.server.wait_disconnect() => {
				bail!("Server should just run in the background");
			}
		);
	}

	/// Send ping and check that a pong is received.
	#[tokio::test]
	async fn test_pong() -> Result<()> {
		let mut state = TestConnection::new()?;
		state.set_connected().await;

		let packet = OutPacket::new_with_dir(
			Direction::S2C,
			Flags::UNENCRYPTED,
			PacketType::Ping,
		);
		state.server.send_packet(packet.clone())?;
		state.server.send_packet(packet.clone())?;

		let (send, recv) = oneshot::channel();
		let send = Cell::new(Some(send));
		let counter = Cell::new(0u8);
		let listener = move |event: &Event| match event {
			Event::ReceivePacket(packet) => {
				if packet.header().packet_type() == PacketType::Pong {
					counter.set(counter.get() + 1);
					// Until 2 pongs are received
					if counter.get() == 2 {
						send.replace(None).unwrap().send(()).unwrap();
					}
				}
			}
			_ => {}
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
		bail!("Unexpected disconnect");
	}

	/// Check that the packet id wraps around.
	#[tokio::test]
	async fn test_generation_id() -> Result<()> {
		let mut state = TestConnection::new()?;
		state.set_connected().await;

		// Sending 70 000 messages takes about 7 minutes in debug mode so we
		// start at 65 500 and send only 100.
		let count = 100;

		// Set current id
		for c in &mut [&mut state.client, &mut state.server] {
			c.codec.outgoing_p_ids[PacketType::Command.to_usize().unwrap()] =
				(0, 65_500);
			c.codec.incoming_p_ids[PacketType::Command.to_usize().unwrap()] =
				(0, 65_500);
			c.codec.outgoing_p_ids[PacketType::Ack.to_usize().unwrap()] =
				(0, 65_500);
			c.codec.incoming_p_ids[PacketType::Ack.to_usize().unwrap()] =
				(0, 65_500);
		}

		let (send, recv) = oneshot::channel();
		let send = Cell::new(Some(send));
		let counter = Cell::new(0u8);
		let listener = move |event: &Event| match event {
			Event::ReceivePacket(packet) => {
				if packet.header().packet_type() == PacketType::Command {
					counter.set(counter.get() + 1);
					if counter.get() == count {
						send.replace(None).unwrap().send(()).unwrap();
					}
				}
			}
			_ => {}
		};

		state.client.event_listeners.push(Box::new(listener));

		for i in 0..count {
			let packet = OutCommand::new::<
				_,
				_,
				String,
				String,
				_,
				_,
				std::iter::Empty<_>,
			>(
				Direction::S2C,
				PacketType::Command,
				"notifytextmessage",
				vec![("msg", format!("message {}", i))].into_iter(),
				std::iter::empty(),
			);

			state.server.send_packet(packet)?;
		}

		tokio::select!(
			r = recv => {
				r?;
				return Ok(());
			}
			_ = state.client.wait_disconnect() => {}
			_ = state.server.wait_disconnect() => {}
		);
		bail!("Unexpected disconnect");
	}
}
