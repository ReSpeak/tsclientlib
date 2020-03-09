use std::net::SocketAddr;
use std::ops::{Deref, DerefMut};
use std::pin::Pin;
use std::task::{Context, Poll};

use anyhow::{bail, format_err};
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
use tokio::net::UdpSocket;
use tsproto_packets::packets::*;

use crate::algorithms as algs;
use crate::connection::{ConnectedParams, Connection, StreamItem};
use crate::crypto::{EccKeyPrivEd25519, EccKeyPrivP256, EccKeyPubP256};
use crate::license::Licenses;
use crate::resend::{Resender, ResenderState};
use crate::Result;

pub struct Client {
	con: Connection,
	pub private_key: EccKeyPrivP256,
}

impl Client {
	pub fn new(
		logger: Logger,
		address: SocketAddr,
		udp_socket: UdpSocket,
		private_key: EccKeyPrivP256,
	) -> Self
	{
		Self {
			con: Connection::new(true, logger, address, udp_socket),
			private_key,
		}
	}

	async fn get_init(&mut self, fut: impl Future<Output=Result<()>>, init_step: u8) -> Result<InS2CInitBuf> {
		let logger = self.logger.clone();
		Ok(tokio::try_join!(fut, self.filter_items(|con, i| Ok(match i {
			StreamItem::S2CInit(packet) => {
				if packet.data().data().get_step() == init_step {
					Some(packet)
				} else {
					// Resent packet
					con.hand_back_buffer(packet.into_buffer());
					None
				}
			}
			StreamItem::C2SInit(packet) => {
				con.hand_back_buffer(packet.into_buffer());
				None
			}
			StreamItem::Error(e) => {
				warn!(logger, "Got connection error"; "error" => %e);
				None
			}
			i => bail!("Unexpected packet, wanted S2CInit instead of {:?}", i),
		})))?.1)
	}

	async fn get_command(&mut self, fut: impl Future<Output=Result<()>>) -> Result<InCommandBuf> {
		let logger = self.logger.clone();
		Ok(tokio::try_join!(fut, self.filter_items(|con, i| Ok(match i {
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
				warn!(logger, "Got connection error"; "error" => %e);
				None
			}
			i => bail!("Unexpected packet, wanted Command instead of {:?}", i),
		})))?.1)
	}

	pub async fn connect(&mut self) -> Result<()> {
		// Send the first init packet
		// Get the current timestamp
		let now = OffsetDateTime::now();
		let timestamp = now.timestamp() as u32;
		let mut rng = rand::thread_rng();

		// Random bytes
		let random0 = rng.gen::<[u8; 4]>();

		// Wait for Init1
		let fut2;
		{
			let fut = self.send_packet(OutC2SInit0::new(timestamp, timestamp, random0)).await;
			let init1 = self.get_init(fut, 1).await?;
			match init1.data().data() {
				S2CInitData::Init1 { random1, random0_r } => {
					// Check the response
					// Most of the time, random0_r is the reversed random0, but
					// sometimes it isn't so do not check it.
					// random0.as_ref().iter().rev().eq(random0_r.as_ref())
					// The packet is correct.

					// Send next init packet
					fut2 = self.send_packet(OutC2SInit2::new(
						timestamp,
						random1,
						**random0_r,
					));
				}
				_ => bail!("Unexpected init packet, needs Init1"),
			}
		}

		// Wait for Init3
		let fut = fut2.await;
		let alpha;
		let fut2;
		{
			let init3 = self.get_init(fut, 3).await?;
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
						let y = x.pow_mod(&e, &n)?;
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
					fut2 = self.send_packet(OutC2SInit4::new(
						timestamp, &x, &n, level, &random2,
						&y, &alpha, &omega, &ip,
					));
				}
				_ => bail!("Unexpected init packet, needs Init3"),
			}
		}

		let fut = fut2.await;
		let fut2;
		{
			let command = self.get_command(fut).await?;
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
			fut2 = self.send_packet(OutCommand::new::<
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
			));
		}
		// Ignore result
		let _ = fut2.await;

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

	// TODO Send messages with return_code and get back future
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
		match Pin::new(&mut **self).poll_next(cx) {
			Poll::Ready(Some(Ok(StreamItem::Command(command)))) => {
				let cmd_name = &command.data().data().name;
				if cmd_name == &"initserver" {
					// Handle an initserver
					let cmd = command.data().iter().next().unwrap();
					if let Some(params) = &mut self.params {
						match cmd.get_parse("aclid") {
							Ok(c_id) => params.c_id = c_id,
							Err(e) => {
								return Poll::Ready(Some(Err(
									format_err!("Failed to parse aclid: {:?}", e))));
							}
						}
					}

					// Notify the resender that we are connected
					let this = &mut **self;
					this.resender.set_state(&this.logger, ResenderState::Connected);
				} else if cmd_name == &"notifyclientleftview" {
					// Handle disconnect
					let cmd = command.data().iter().next().unwrap();
					if let Some(params) = &mut self.params {
						if cmd.get_parse("clid") == Ok(params.c_id) {
							// We are disconnecting
							let this = &mut **self;
							this.resender.set_state(&this.logger, ResenderState::Disconnecting);
						}
					}
				} else if cmd_name == &"notifyplugincmd" {
					let cmd = command.data().iter().next().unwrap();
					// TODO getversion
					if cmd.get("name") == Some("getversion") && cmd.has("client") {
						if let Ok(sender) = cmd.get("client").unwrap().parse::<u16>() {
							let mut version = format!(
								"{} {}",
								env!("CARGO_PKG_NAME"),
								git_testament::render_testament!(crate::TESTAMENT),
							);
							#[cfg(debug_assertions)]
							version.push_str(" (Debug)");
							#[cfg(not(debug_assertions))]
							version.push_str(" (Release)");

							let _p = Some(OutCommand::new::<
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
							));
						}
					}
				}

				Poll::Ready(Some(Ok(StreamItem::Command(command))))
			}
			r => r,
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	use num_traits::ToPrimitive;
	use slog::{o, Drain};
	use tokio::runtime::current_thread::Runtime;
	use tokio::util::FutureExt;

	pub struct TestPacketHandler;

	impl<T: 'static> PacketHandler<T> for TestPacketHandler {
		fn new_connection<S1, S2, S3, S4>(
			&mut self,
			_con_val: &ConnectionValue<T>,
			s2c_init_stream: S1,
			_c2s_init_stream: S2,
			command_stream: S3,
			audio_stream: S4,
		) where
			S1: Stream<Item = InS2CInit, Error = Error> + Send + 'static,
			S2: Stream<Item = InC2SInit, Error = Error> + Send + 'static,
			S3: Stream<Item = InCommand, Error = Error> + Send + 'static,
			S4: Stream<Item = InAudio, Error = Error> + Send + 'static,
		{
			tokio::spawn(
				s2c_init_stream
					.for_each(|_| Ok(()))
					.map_err(|e| panic!("s2c_init_stream errored: {:?}", e)),
			);
			tokio::spawn(
				command_stream
					.for_each(|_| Ok(()))
					.map_err(|e| panic!("command_stream errored: {:?}", e)),
			);
			tokio::spawn(
				audio_stream
					.for_each(|_| Ok(()))
					.map_err(|e| panic!("audio_stream errored: {:?}", e)),
			);
		}
	}

	pub struct TestConnection {
		pub client: Arc<Mutex<ClientData<TestPacketHandler>>>,
		pub client_con: ClientConVal,
		// Mocked "server" to send and receive packets
		pub server: Arc<Mutex<ClientData<TestPacketHandler>>>,
		pub server_con: ClientConVal,
	}

	impl TestConnection {
		pub fn new() -> (Runtime, Self) {
			let mut runtime = Runtime::new().unwrap();

			let (send_sink, send_stream) =
				mpsc::unbounded::<(Bytes, SocketAddr)>();
			let (recv_sink, recv_stream) =
				mpsc::unbounded::<(BytesMut, SocketAddr)>();

			let logger = {
				// TODO Write to stdout, somehow does not work
				//let decorator = slog_term::PlainDecorator::new(std::io::stdout());
				let decorator =
					slog_term::TermDecorator::new().stdout().build();
				let drain =
					slog_term::FullFormat::new(decorator).build().fuse();
				let drain = slog_async::Async::new(drain).build().fuse();

				slog::Logger::root(drain, o!())
			};

			let res = runtime
				.block_on(future::lazy(|| -> Result<_> {
					// Create client
					let c = ClientData::new_with_socket(
						"127.0.0.1:0".parse().unwrap(),
						EccKeyPrivP256::create().unwrap(),
						true,
						None,
						DefaultPacketHandler::new(TestPacketHandler),
						SocketConnectionManager::new(),
						send_sink,
						recv_stream,
						logger.clone(),
					)
					.expect("Failed to create client");

					let c2 = Arc::downgrade(&c);
					{
						let mut c = c.lock().unwrap();
						let c = &mut *c;
						// Set the data reference
						c.packet_handler.complete(c2.clone());

						//crate::log::add_udp_packet_logger(c);
						// Change state on disconnect
						c.add_out_packet_observer(
							"tsproto::client".into(),
							Box::new(ClientOutPacketObserver),
						);
					}

					// Create "server"
					let s = ClientData::new_with_socket(
						"127.0.0.1:0".parse().unwrap(),
						EccKeyPrivP256::create().unwrap(),
						false,
						None,
						DefaultPacketHandler::new(TestPacketHandler),
						SocketConnectionManager::new(),
						recv_sink
							.sink_map_err(|e| {
								format_err!(
									"Failed to send from server: {:?}",
									e
								)
							})
							.with(|(b, a): (Bytes, SocketAddr)| -> Result<_> {
								Ok((b.into(), a))
							}),
						send_stream.map(|(b, a)| (b.into(), a)),
						logger.new(o!("server" => true)),
					)
					.expect("Failed to create \"server\" mock");

					let s2 = Arc::downgrade(&s);
					{
						let mut c = s.lock().unwrap();
						let c = &mut *c;
						// Set the data reference
						c.packet_handler.complete(s2.clone());
						//crate::log::add_udp_packet_logger(c);
					}

					let client_con = Self::set_connected(&c);
					let server_con = Self::set_connected(&s);
					let res =
						Self { client: c, server: s, client_con, server_con };

					Ok(res)
				}))
				.unwrap();

			(runtime, res)
		}

		/// Set the connection to connected.
		fn set_connected(
			data: &Arc<Mutex<ClientData<TestPacketHandler>>>,
		) -> ClientConVal {
			let mut client = data.lock().unwrap();
			let con_key = client.add_connection(
				Arc::downgrade(&data),
				ServerConnectionData {
					state_change_listener: Vec::new(),
					state: ServerConnectionState::Connected,
				},
				"127.0.0.1:1".parse().unwrap(),
			);
			let connection = client.get_connection(&con_key).unwrap();
			let mut con = connection.mutex.lock().unwrap();
			con.1.resender.handle_event(ResenderEvent::Connected);
			con.1.outgoing_p_ids[PacketType::Command.to_usize().unwrap()] =
				(0, 1);
			con.1.incoming_p_ids[PacketType::Command.to_usize().unwrap()] =
				(0, 1);
			con.1.outgoing_p_ids[PacketType::Ack.to_usize().unwrap()] = (0, 1);
			con.1.incoming_p_ids[PacketType::Ack.to_usize().unwrap()] = (0, 1);

			// Set params
			con.1.params = Some(ConnectedParams::new(
				client.private_key.to_pub(),
				SharedIv::Protocol31([0; 64]),
				[0; 8],
			));
			let params = con.1.params.as_mut().unwrap();
			params.c_id = 1;

			connection.downgrade()
		}

		/// Encrypts the packet content and sends it to the connection.
		pub fn send_packet(
			&self,
			packet: OutPacket,
		) -> impl Future<Item = (), Error = Error>
		{
			self.server_con
				.as_packet_sink()
				.send(packet)
				.map_err(|e| panic!("Failed to send simulated packet: {:?}", e))
				.map(|_| ())
		}
	}

	struct InitObserver(mpsc::UnboundedSender<()>);
	impl<T> InPacketObserver<T> for InitObserver {
		fn observe(&self, _: &mut (T, Connection), packet: &InPacket) {
			let header = packet.header();
			if header.packet_type() == PacketType::Init {
				tokio::spawn(
					self.0
						.clone()
						.send(())
						.map(|_| ())
						.map_err(|e| panic!("Failed to send: {:?}", e)),
				);
			}
		}
	}

	#[tokio::test]
	async fn test_connect_timeout() -> Result<()> {
		let (mut runtime, con) = TestConnection::new();
		let cw = Arc::downgrade(&con.client);
		// Add observer
		let (send, recv) = mpsc::unbounded();
		con.server.lock().unwrap().add_in_packet_observer(
			"tsproto::test".into(),
			Box::new(InitObserver(send)),
		);

		let r = connect(
			cw,
			&mut *con.client.lock().unwrap(),
			"127.0.0.1:1".parse().unwrap(),
		);
		r.map(|_| panic!("Should not connect")).then(|_| {
			// Drop client in the end
			drop(con);
			recv.into_future()
				.map(|_| ())
				.map_err(|_| panic!("Failed to receive init packet"))
		});

		Ok(())
	}

	struct PongObserver(mpsc::UnboundedSender<()>);
	impl<T> InPacketObserver<T> for PongObserver {
		fn observe(&self, _: &mut (T, Connection), packet: &InPacket) {
			let header = packet.header();
			if header.packet_type() == PacketType::Pong
				&& header.packet_id() == 1
			{
				tokio::spawn(
					self.0
						.clone()
						.send(())
						.map(|_| ())
						.map_err(|e| panic!("Failed to send: {:?}", e)),
				);
			}
		}
	}

	/// Send ping and check that a pong is received.
	#[test]
	fn test_pong() {
		let (mut runtime, con) = TestConnection::new();

		runtime
			.block_on(future::lazy(|| {
				let packet = OutPacket::new_with_dir(
					Direction::S2C,
					Flags::UNENCRYPTED,
					PacketType::Ping,
				);
				tokio::spawn(
					con.send_packet(packet.clone())
						.map_err(|e| panic!("Failed to send packet: {:?}", e)),
				);
				tokio::spawn(
					con.send_packet(packet)
						.map_err(|e| panic!("Failed to send packet: {:?}", e)),
				);

				// Add observer
				let (send, recv) = mpsc::unbounded();
				con.server.lock().unwrap().add_in_packet_observer(
					"tsproto::test".into(),
					Box::new(PongObserver(send)),
				);
				recv.into_future()
					.map(|_| drop(con))
					.timeout(Duration::from_secs(5))
					.map_err(|_| panic!("Failed to receive pong"))
			}))
			.unwrap();
	}

	struct CounterObserver(mpsc::UnboundedSender<()>, Mutex<usize>);
	impl<T> InPacketObserver<T> for CounterObserver {
		fn observe(&self, _: &mut (T, Connection), packet: &InPacket) {
			let header = packet.header();
			if header.packet_type() == PacketType::Command
				&& *self.1.lock().unwrap() == 0
			{
				tokio::spawn(
					self.0
						.clone()
						.send(())
						.map(|_| ())
						.map_err(|e| panic!("Failed to send: {:?}", e)),
				);
			} else {
				*self.1.lock().unwrap() -= 1;
			}
		}
	}

	#[test]
	fn test_generation_id() {
		let (mut runtime, con) = TestConnection::new();

		// Set current id
		for c in &[&con.client_con, &con.server_con] {
			let c = c.upgrade().unwrap();
			let mut c = c.mutex.lock().unwrap();
			c.1.outgoing_p_ids[PacketType::Command.to_usize().unwrap()] =
				(0, 65_000);
			c.1.incoming_p_ids[PacketType::Command.to_usize().unwrap()] =
				(0, 65_000);
			c.1.outgoing_p_ids[PacketType::Ack.to_usize().unwrap()] =
				(0, 65_000);
			c.1.incoming_p_ids[PacketType::Ack.to_usize().unwrap()] =
				(0, 65_000);
		}

		runtime.spawn(future::lazy(move || {
			// Sending 70 000 messages takes about 7 minutes (in debug mode) so
			// we start at 65 000 and send only 5 000
			let mut msgs = Vec::new();
			let count = 5_000;
			let (send, recv) = mpsc::unbounded();
			con.client.lock().unwrap().add_in_packet_observer(
				"tsproto::test".into(),
				Box::new(CounterObserver(send, Mutex::new(count - 1))),
			);

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
				msgs.push(
					con.send_packet(packet.clone())
						.map_err(|e| panic!("Failed to send packet: {:?}", e)),
				);
			}

			tokio::spawn(stream::futures_ordered(msgs).for_each(|_| Ok(())));

			recv.into_future()
				.map(|_| drop(con))
				.map_err(|_| panic!("Failed to receive all packets"))
		}));
		runtime.run().unwrap();
	}
}
