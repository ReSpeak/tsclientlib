use std::io::Cursor;
use std::net::SocketAddr;
use std::u16;

use bytes::Bytes;
use futures::sync::mpsc;
use futures::{future, Future, IntoFuture, Sink};
use num_traits::ToPrimitive;
use slog::Logger;
use tokio;

use crate::algorithms as algs;
use crate::connection::Connection;
use crate::connectionmanager::{ConnectionManager, Resender};
use crate::handler_data::{
	ConnectionValue, Data, InCommandObserver, InPacketObserver,
};
use crate::packets::Data as PData;
use crate::packets::*;
use crate::{
	Error, LockedHashMap, Result, MAX_FRAGMENTS_LENGTH, MAX_QUEUE_LEN,
};

/// Decodes incoming udp packets.
///
/// This part does the defragmentation, decryption and decompression.
pub struct PacketCodecReceiver<CM: ConnectionManager + 'static> {
	connections: LockedHashMap<CM::Key, ConnectionValue<CM::AssociatedData>>,
	is_client: bool,
	logger: Logger,
	in_packet_observer:
		LockedHashMap<String, Box<InPacketObserver<CM::AssociatedData>>>,
	in_command_observer:
		LockedHashMap<String, Box<InCommandObserver<CM::AssociatedData>>>,

	/// The sink for `UdpPacket`s with no known connection.
	///
	/// This can stay `None` so all packets without connection will be dropped.
	unknown_udp_packet_sink: Option<mpsc::Sender<(SocketAddr, InPacket)>>,
}

impl<CM: ConnectionManager + 'static> PacketCodecReceiver<CM> {
	pub fn new(
		data: &Data<CM>,
		unknown_udp_packet_sink: Option<mpsc::Sender<(SocketAddr, InPacket)>>,
	) -> Self
	{
		Self {
			connections: data.connections.clone(),
			is_client: data.is_client,
			logger: data.logger.clone(),
			in_packet_observer: data.in_packet_observer.clone(),
			in_command_observer: data.in_command_observer.clone(),
			unknown_udp_packet_sink,
		}
	}

	pub fn handle_udp_packet(
		&mut self,
		(addr, packet): (SocketAddr, InPacket),
	) -> impl Future<Item = (), Error = Error>
	{
		// Find the right connection
		let cons = self.connections.read();
		if let Some(con) =
			cons.get(&CM::get_connection_key(addr, &packet)).cloned()
		{
			// If we are a client and have only a single connection, we will do the
			// work inside this future and not spawn a new one.
			let logger = self.logger.new(o!("addr" => addr));
			let in_packet_observer = self.in_packet_observer.clone();
			let in_command_observer = self.in_command_observer.clone();
			if self.is_client && cons.len() == 1 {
				drop(cons);
				Self::connection_handle_udp_packet(
					&logger,
					in_packet_observer,
					in_command_observer,
					self.is_client,
					&con,
					addr,
					packet,
				)
				.into_future()
			} else {
				drop(cons);
				let is_client = self.is_client;
				tokio::spawn(future::lazy(move || {
					if let Err(e) = Self::connection_handle_udp_packet(
						&logger,
						in_packet_observer,
						in_command_observer,
						is_client,
						&con,
						addr,
						packet,
					) {
						error!(logger, "Error handling udp packet"; "error" => ?e);
					}
					Ok(())
				}));
				future::ok(())
			}
		} else {
			drop(cons);
			// Unknown connection
			if let Some(sink) = &mut self.unknown_udp_packet_sink {
				// Don't block if the queue is full
				if sink.try_send((addr, packet)).is_err() {
					warn!(self.logger, "Unknown connection handler overloaded \
						– dropping udp packet");
				}
			} else {
				warn!(
					self.logger,
					"Dropped packet without connection because no unknown \
					 packet handler is set"
				);
			}
			future::ok(())
		}
	}

	/// Handle a packet for a specific connection.
	///
	/// This part does the defragmentation, decryption and decompression.
	fn connection_handle_udp_packet(
		logger: &Logger,
		in_packet_observer: LockedHashMap<
			String,
			Box<InPacketObserver<CM::AssociatedData>>,
		>,
		in_command_observer: LockedHashMap<
			String,
			Box<InCommandObserver<CM::AssociatedData>>,
		>,
		is_client: bool,
		connection: &ConnectionValue<CM::AssociatedData>,
		_: SocketAddr,
		mut packet: InPacket,
	) -> Result<()>
	{
		let con2 = connection.downgrade();
		let mut con = connection.mutex.lock();
		let con = &mut *con;
		let packet_res;
		let mut ack = None;

		let p_type = packet.header().packet_type();
		let type_i = p_type.to_usize().unwrap();
		let id = packet.header().packet_id();
		let (in_recv_win, gen_id, cur_next, limit) =
			con.1.in_receive_window(p_type, id);

		if con.1.params.is_some() && p_type == PacketType::Init {
			return Err(Error::UnexpectedInitPacket);
		}

		// Ignore range for acks
		if p_type == PacketType::Ack
			|| p_type == PacketType::AckLow
			|| in_recv_win
		{
			if !packet.header().flags().contains(Flags::UNENCRYPTED) {
				// If it is the first ack packet of a client, try to fake
				// decrypt it.
				let decrypted = if (p_type == PacketType::Ack
					&& id <= 1 && is_client)
					|| con.1.params.is_none()
				{
					if let Ok(dec) = algs::decrypt_fake(&packet) {
						packet.set_content(dec);
						true
					} else {
						false
					}
				} else {
					false
				};
				if !decrypted {
					if let Some(params) = &mut con.1.params {
						// Decrypt the packet
						let dec_res = algs::decrypt(
							&packet,
							gen_id,
							&params.shared_iv,
							&mut params.key_cache,
						);
						if dec_res.is_err()
							&& p_type == PacketType::Ack
							&& id == 1 && is_client
						{
							// Ignore error, this is the ack packet for the
							// clientinit, we take the initserver as ack anyway.
							return Ok(());
						}

						packet.set_content(dec_res?);
					} else {
						// Failed to fake decrypt the packet
						return Err(Error::WrongMac);
					}
				}
			} else if algs::must_encrypt(p_type) {
				// Check if it is ok for the packet to be unencrypted
				return Err(Error::UnallowedUnencryptedPacket);
			}

			for o in in_packet_observer.read().values() {
				o.observe(con, &packet);
			}

			let in_ids = &mut con.1.incoming_p_ids;
			match p_type {
				PacketType::Command | PacketType::CommandLow => {
					if p_type == PacketType::Command {
						ack = Some((PacketType::Ack, PData::Ack(id)));
					} else if p_type == PacketType::CommandLow {
						ack = Some((PacketType::AckLow, PData::AckLow(id)));
					}
					let r_queue = &mut con.1.receive_queue;
					let frag_queue = &mut con.1.fragmented_queue;

					let commands = Self::handle_command_packet(
						logger, r_queue, frag_queue, in_ids, packet,
					)?;

					// Be careful with command packets, they are
					// guaranteed to be in the right order now, because
					// we hold a lock on the connection.
					for c in commands {
						for o in in_command_observer.read().values() {
							o.observe(con, &c);
						}

						// Send to packet handler
						if let Err(e) = con.1.command_sink.unbounded_send(c) {
							error!(logger, "Failed to send command packet to \
								handler"; "error" => ?e);
						}
					}

					// Dummy value
					packet_res = Ok(None);
				}
				_ => {
					if p_type == PacketType::Ping {
						ack = Some((PacketType::Pong, PData::Pong(id)));
					}
					// Update packet ids
					let (id, next_gen) = id.overflowing_add(1);
					if p_type != PacketType::Init {
						in_ids[type_i] =
							(if next_gen { gen_id + 1 } else { gen_id }, id);
					}

					if let Some(ack_id) = packet.ack_packet() {
						// Remove command packet from send queue if the fitting ack is received.
						let p_type = if p_type == PacketType::Ack {
							PacketType::Command
						} else {
							PacketType::CommandLow
						};
						con.1.resender.ack_packet(p_type, ack_id);
					} else if p_type.is_voice() {
						// Seems to work better without assembling the first 3 voice packets
						// Use handle_voice_packet to assemble fragmented voice packets
						/*let mut res = Self::handle_voice_packet(&logger, params, &header, p_data);
						let res = res.drain(..).map(|p|
							(con_key.clone(), p)).collect();
						Ok(res)*/
					}
					packet_res = Ok(Some(packet));
				}
			}
		} else {
			// Send an ack for the case when it was lost
			if p_type == PacketType::Command {
				ack = Some((PacketType::Ack, PData::Ack(id)));
			} else if p_type == PacketType::CommandLow {
				ack = Some((PacketType::AckLow, PData::AckLow(id)));
			}
			packet_res = Err(Error::NotInReceiveWindow {
				id,
				next: cur_next,
				limit,
				p_type,
			});
		};

		// Send ack
		if let Some((ack_type, ack_packet)) = ack {
			let mut ack_header = Header::default();
			ack_header.set_type(ack_type);
			tokio::spawn(
				con2.as_packet_sink()
					.send(Packet::new(ack_header, ack_packet))
					.map(|_| ())
					// Ignore errors, this can happen if the connection is
					// already gone because we are disconnected.
					// TODO Wait until the last ack is sent before disconnecting
					.map_err(|_| ()),
			);
		}

		if let Some(packet) = packet_res? {
			if p_type.is_voice() {
				if let Err(e) =
					con.1.audio_sink.unbounded_send(packet.into_audio()?)
				{
					error!(logger, "Failed to send packet to handler"; "error" => ?e);
				}
			} else if p_type == PacketType::Init {
				if is_client {
					if let Err(e) = con
						.1
						.s2c_init_sink
						.unbounded_send(packet.into_s2cinit()?)
					{
						error!(logger, "Failed to send packet to handler"; "error" => ?e);
					}
				} else if let Err(e) =
					con.1.c2s_init_sink.unbounded_send(packet.into_c2sinit()?)
				{
					error!(logger, "Failed to send packet to handler"; "error" => ?e);
				}
			}
		}
		Ok(())
	}

	/// Handle `Command` and `CommandLow` packets.
	///
	/// They have to be handled in the right order.
	fn handle_command_packet(
		logger: &Logger,
		r_queue: &mut [Vec<InPacket>; 2],
		frag_queue: &mut [Option<(InPacket, Vec<u8>)>; 2],
		in_ids: &mut [(u32, u16); 8],
		mut packet: InPacket,
	) -> Result<Vec<InCommand>>
	{
		let header = packet.header();
		let p_type = header.packet_type();
		let mut id = header.packet_id();
		let type_i = p_type.to_usize().unwrap();
		let cmd_i = if p_type == PacketType::Command { 0 } else { 1 };
		let r_queue = &mut r_queue[cmd_i];
		let frag_queue = &mut frag_queue[cmd_i];
		let in_ids = &mut in_ids[type_i];
		let cur_next = in_ids.1;
		if cur_next == id {
			// In order
			let mut packets = Vec::new();
			loop {
				// Update next packet id
				let (next_id, next_gen) = id.overflowing_add(1);
				if next_gen {
					// Next packet generation
					in_ids.0 = in_ids.0.wrapping_add(1);
				}
				in_ids.1 = next_id;

				let flags = packet.header().flags();
				let res_packet = if flags.contains(Flags::FRAGMENTED) {
					if let Some((header, mut frag_queue)) = frag_queue.take() {
						// Last fragmented packet
						frag_queue.extend_from_slice(packet.content());
						// Decompress
						let decompressed = if header
							.header()
							.flags()
							.contains(Flags::COMPRESSED)
						{
							//debug!(logger, "Compressed"; "data" => ?::utils::HexSlice(&frag_queue));
							::quicklz::decompress(
								&mut Cursor::new(frag_queue),
								crate::MAX_DECOMPRESSED_SIZE,
							)?
						} else {
							frag_queue
						};
						/*if header.get_compressed() {
							debug!(logger, "Decompressed";
								"data" => ?::HexSlice(&decompressed),
								"string" => %String::from_utf8_lossy(&decompressed),
							);
						}*/
						Some(InCommand::with_content(&header, decompressed)?)
					} else {
						// Enqueue
						let content = packet.take_content();
						*frag_queue = Some((packet, content));
						None
					}
				} else if let Some((_, ref mut frag_queue)) = *frag_queue {
					// The packet is fragmented
					if frag_queue.len() < MAX_FRAGMENTS_LENGTH {
						frag_queue.extend_from_slice(packet.content());
						None
					} else {
						return Err(Error::MaxLengthExceeded(String::from(
							"fragment queue",
						)));
					}
				} else {
					// Decompress
					let decompressed = if flags.contains(Flags::COMPRESSED) {
						//debug!(logger, "Compressed"; "data" => ?::utils::HexSlice(packet.content()));
						::quicklz::decompress(
							&mut Cursor::new(packet.content()),
							crate::MAX_DECOMPRESSED_SIZE,
						)?
					} else {
						packet.take_content()
					};
					/*if header.get_compressed() {
						debug!(logger, "Decompressed"; "data" => ?::HexSlice(&decompressed));
					}*/
					Some(InCommand::with_content(&packet, decompressed)?)
				};
				if let Some(p) = res_packet {
					packets.push(p);
				}

				// Check if there are following packets in the receive queue.
				id = id.wrapping_add(1);
				if let Some(pos) =
					r_queue.iter().position(|p| p.header().packet_id() == id)
				{
					packet = r_queue.remove(pos);
				} else {
					break;
				}
			}
			// The first packets should be returned first
			packets.reverse();
			Ok(packets)
		} else {
			// Out of order
			warn!(logger, "Out of order command packet"; "got" => id,
				"expected" => cur_next);
			let limit = ((u32::from(cur_next) + MAX_QUEUE_LEN as u32)
				% u32::from(u16::MAX)) as u16;
			if (cur_next < limit && id >= cur_next && id < limit)
				|| (cur_next > limit && (id >= cur_next || id < limit))
			{
				r_queue.push(packet);
				Ok(vec![])
			} else {
				Err(Error::MaxLengthExceeded(String::from("command queue")))
			}
		}
	}

	/*/// Handle `Voice` and `VoiceLow` packets.
	///
	/// The first 3 packets for each audio transmission have the compressed flag
	/// set, which means they are fragmented and should be concatenated.
	fn handle_voice_packet(
		logger: &slog::Logger,
		params: &mut ConnectedParams,
		header: &Header,
		packet: PData,
	) -> Vec<Packet> {
		let cmd_i = if header.get_type() == PacketType::Voice {
			0
		} else {
			1
		};
		let frag_queue = &mut params.voice_fragmented_queue[cmd_i];

		let (id, from_id, codec_type, voice_data) = match packet {
			PData::VoiceS2C { id, from_id, codec_type, voice_data } => (id, from_id, codec_type, voice_data),
			PData::VoiceWhisperS2C { id, from_id, codec_type, voice_data } => (id, from_id, codec_type, voice_data),
			_ => unreachable!("handle_voice_packet did get an unknown voice packet"),
		};

		if header.get_compressed() {
			let queue = frag_queue.entry(from_id).or_insert_with(Vec::new);
			// Append to fragments
			if queue.len() < MAX_FRAGMENTS_LENGTH {
				queue.extend_from_slice(&voice_data);
				return Vec::new();
			}
			warn!(logger, "Length of voice fragment queue exceeded"; "len" => queue.len());
		}

		let mut res = Vec::new();
		if let Some(frags) = frag_queue.remove(&from_id) {
			// We got two packets
			let packet_data = if header.get_type() == PacketType::Voice {
				PData::VoiceS2C { id, from_id, codec_type, voice_data: frags }
			} else {
				PData::VoiceWhisperS2C { id, from_id, codec_type, voice_data: frags }
			};
			res.push(Packet::new(header.clone(), packet_data));
		}

		let packet_data = if header.get_type() == PacketType::Voice {
			PData::VoiceS2C { id, from_id, codec_type, voice_data }
		} else {
			PData::VoiceWhisperS2C { id, from_id, codec_type, voice_data }
		};
		res.push(Packet::new(header.clone(), packet_data));
		res
	}*/
}

/// Encodes outgoing packets.
///
/// This part does the compression, encryption and fragmentation.
pub struct PacketCodecSender {
	is_client: bool,
}

impl PacketCodecSender {
	pub fn new(is_client: bool) -> Self { Self { is_client } }

	pub fn encode_packet(
		&self,
		con: &mut Connection,
		mut packet: Packet,
	) -> Result<Vec<(u16, Bytes)>>
	{
		let p_type = packet.header.get_type();
		let type_i = p_type.to_usize().unwrap();

		let use_newprotocol = (p_type == PacketType::Command
			|| p_type == PacketType::CommandLow)
			&& self.is_client;

		// Change state on disconnect
		if let PData::Command(cmd) = &packet.data {
			if cmd.command == "clientdisconnect" {
				con.resender.handle_event(
					crate::connectionmanager::ResenderEvent::Disconnecting,
				);
			}
		}

		let (gen, p_id) = if p_type == PacketType::Init {
			(0, 0)
		} else {
			con.outgoing_p_ids[type_i]
		};
		// We fake encrypt the first command packet of the
		// server (id 0) and the first command packet of the
		// client (id 1) if the client uses the new protocol
		// (the packet is a clientek).
		let mut fake_encrypt = p_type == PacketType::Command
			&& gen == 0
			&& ((!self.is_client && p_id == 0)
				|| (self.is_client && p_id == 1 && {
					// Test if it is a clientek packet
					if let PData::Command(ref cmd) = packet.data {
						cmd.command == "clientek"
					} else {
						false
					}
				}));

		// Compress and split packet
		let packet_id;
		let mut packets = if p_type == PacketType::Command
			|| p_type == PacketType::CommandLow
		{
			packet_id = None;
			algs::compress_and_split(self.is_client, &packet)
		} else {
			// Set the inner packet id for voice packets
			match packet.data {
				PData::VoiceC2S { ref mut id, .. }
				| PData::VoiceS2C { ref mut id, .. }
				| PData::VoiceWhisperC2S { ref mut id, .. }
				| PData::VoiceWhisperNewC2S { ref mut id, .. }
				| PData::VoiceWhisperS2C { ref mut id, .. } => {
					*id = con.outgoing_p_ids[type_i].1;
				}
				_ => {}
			}

			// Identify init packets by their number
			packet_id = match packet.data {
				PData::C2SInit(C2SInit::Init0 { .. }) => Some(0),
				PData::C2SInit(C2SInit::Init2 { .. }) => Some(2),
				PData::C2SInit(C2SInit::Init4 { .. }) => Some(4),
				PData::S2CInit(S2CInit::Init1 { .. }) => Some(1),
				PData::S2CInit(S2CInit::Init3 { .. }) => Some(3),
				_ => None,
			};

			let mut data = Vec::new();
			packet.data.write(&mut data).unwrap();
			vec![(packet.header, data)]
		};

		// Get values from parameters
		let should_encrypt;
		let c_id;
		if let Some(params) = con.params.as_mut() {
			should_encrypt =
				algs::should_encrypt(p_type, params.voice_encryption);
			c_id = params.c_id;
		} else {
			should_encrypt = algs::should_encrypt(p_type, false);
			if should_encrypt {
				fake_encrypt = true;
			}
			c_id = 0;
		}

		let packets = packets
			.drain(..)
			.map(|(mut header, mut p_data)| -> Result<_> {
				// Packet data (without header)
				// Get packet id
				let (mut gen, mut p_id) = if p_type == PacketType::Init {
					(0, 0)
				} else {
					con.outgoing_p_ids[type_i]
				};
				if p_type != PacketType::Init {
					header.p_id = p_id;
				}

				// Identify init packets by their number
				let packet_id = if let Some(id) = packet_id {
					id
				} else {
					header.p_id
				};

				// Set newprotocol flag if needed
				if use_newprotocol {
					header.set_newprotocol(true);
				}

				// Client id for clients
				if self.is_client {
					header.c_id = Some(c_id);
				} else {
					header.c_id = None;
				}

				// Encrypt if necessary
				header.set_unencrypted(false);
				if fake_encrypt {
					p_data = algs::encrypt_fake(&mut header, &p_data)?;
				} else if should_encrypt {
					// The params are set
					let params = con.params.as_mut().unwrap();
					p_data = algs::encrypt(
						&mut header,
						&p_data,
						gen,
						&params.shared_iv,
						&mut params.key_cache,
					)?;
				} else {
					header.set_unencrypted(true);
					if let Some(params) = con.params.as_mut() {
						header.mac.copy_from_slice(&params.shared_mac);
					}
				}

				// Increment outgoing_p_ids
				p_id = p_id.wrapping_add(1);
				if p_id == 0 {
					gen = gen.wrapping_add(1);
				}
				if p_type != PacketType::Init {
					con.outgoing_p_ids[type_i] = (gen, p_id);
				}
				let mut buf = Vec::new();
				header.write(&mut buf)?;
				buf.append(&mut p_data);
				Ok((packet_id, buf.into()))
			})
			.collect::<Result<Vec<_>>>()?;
		Ok(packets)
	}
}
