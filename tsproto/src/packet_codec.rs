use std::io::Cursor;
use std::net::SocketAddr;
use std::u16;

use byteorder::{NetworkEndian, WriteBytesExt};
use bytes::Bytes;
use futures::sync::mpsc;
use futures::{future, Future, IntoFuture, Sink};
use num_traits::ToPrimitive;
use slog::{error, o, warn, Logger};
use tokio;
use tsproto_packets::packets::*;

use crate::algorithms as algs;
use crate::connection::Connection;
use crate::connectionmanager::{ConnectionManager, Resender};
use crate::handler_data::{
	ConnectionValue, Data, InCommandObserver, InPacketObserver,
};
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
		LockedHashMap<String, Box<dyn InPacketObserver<CM::AssociatedData>>>,
	in_command_observer:
		LockedHashMap<String, Box<dyn InCommandObserver<CM::AssociatedData>>>,

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
			// TODO Testing, command packets get handled in the wrong order if
			// we spawn a new future for each packet.
			// Send it to a channel per connection.
			if self.is_client && cons.len() == 1 || true {
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
					warn!(
						self.logger,
						"Unknown connection handler overloaded – dropping udp \
						 packet"
					);
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
			Box<dyn InPacketObserver<CM::AssociatedData>>,
		>,
		in_command_observer: LockedHashMap<
			String,
			Box<dyn InCommandObserver<CM::AssociatedData>>,
		>,
		is_client: bool,
		connection: &ConnectionValue<CM::AssociatedData>,
		_: SocketAddr,
		mut packet: InPacket,
	) -> Result<()>
	{
		let con2 = connection.downgrade();
		let packet_sink = con2.as_packet_sink();
		let mut con = connection.mutex.lock();
		let con = &mut *con;
		let packet_res;
		let mut ack = false;

		let p_type = packet.header().packet_type();
		let dir = packet.direction();
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
							&& p_type == PacketType::Ack && id == 1
							&& is_client
						{
							// Ignore error, this is the ack packet for the
							// clientinit, we take the initserver as ack anyway.
							return Ok(());
						}

						packet.set_content(dec_res?);
					} else {
						// Failed to fake decrypt the packet
						return Err(Error::WrongMac(p_type, gen_id, id));
					}
				}
			} else if algs::must_encrypt(p_type) {
				// Check if it is ok for the packet to be unencrypted
				return Err(Error::UnallowedUnencryptedPacket);
			}

			match p_type {
				PacketType::Command | PacketType::CommandLow => {
					ack = true;

					for o in in_packet_observer.read().values() {
						o.observe(con, &packet);
					}
					let in_ids = &mut con.1.incoming_p_ids;

					let r_queue = &mut con.1.receive_queue;
					let frag_queue = &mut con.1.fragmented_queue;

					let commands = Self::handle_command_packet(
						logger, r_queue, frag_queue, in_ids, packet,
					)?;

					// Be careful with command packets, they are
					// guaranteed to be in the right order now, because
					// we hold a lock on the connection.
					let observer = in_command_observer.read();
					for c in commands {
						for o in observer.values() {
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
						ack = true;
					}
					// Update packet ids
					let in_ids = &mut con.1.incoming_p_ids;
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

					// Call observer after handling acks
					for o in in_packet_observer.read().values() {
						o.observe(con, &packet);
					}

					packet_res = Ok(Some(packet));
				}
			}
		} else {
			// Send an ack for the case when it was lost
			if p_type == PacketType::Command || p_type == PacketType::CommandLow
			{
				ack = true;
			}
			packet_res = Err(Error::NotInReceiveWindow {
				id,
				next: cur_next,
				limit,
				p_type,
			});
		};

		// Send ack
		if ack {
			tokio::spawn(
				packet_sink
					.send(OutAck::new(dir.reverse(), p_type, id))
					.map(|_| ())
					// Ignore errors, this can happen if the connection is
					// already gone because we are disconnected.
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
				} else if let Err(e) = con
					.1
					.c2s_init_sink
					.unbounded_send(packet.into_c2sinit().map_err(|(_, e)| e)?)
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
						Some(
							InCommand::with_content(&header, decompressed)
								.map_err(|(_, e)| e)?,
						)
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
					Some(
						InCommand::with_content(&packet, decompressed)
							.map_err(|(_, e)| e)?,
					)
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
			let (limit, next_gen) = cur_next.overflowing_add(MAX_QUEUE_LEN);
			if (!next_gen && id >= cur_next && id < limit)
				|| (next_gen && (id >= cur_next || id < limit))
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
		mut packet: OutPacket,
	) -> Result<Vec<(u32, u16, Bytes)>>
	{
		let p_type = packet.header().packet_type();
		let type_i = p_type.to_usize().unwrap();

		// TODO Needed, commands should set their own flag?
		if (p_type == PacketType::Command || p_type == PacketType::CommandLow)
			&& self.is_client
		{
			// Set newprotocol flag
			packet.flags(packet.header().flags() | Flags::NEWPROTOCOL);
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
			&& gen == 0 && ((!self.is_client && p_id == 0)
			|| (self.is_client && p_id == 1 && {
				// Test if it is a clientek packet
				let s = b"clientek";
				packet.content().len() >= s.len()
					&& packet.content()[..s.len()] == s[..]
			}));
		// Also fake encrypt the first ack of the client, which is the response
		// for the initivexpand2 packet.
		fake_encrypt |= self.is_client
			&& p_type == PacketType::Ack
			&& gen == 0 && p_id == 0;

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

		// Client id for clients
		if self.is_client {
			packet.client_id(c_id);
		}

		if !should_encrypt && !fake_encrypt {
			packet.flags(packet.header().flags() | Flags::UNENCRYPTED);
			if let Some(params) = con.params.as_mut() {
				packet.mac().copy_from_slice(&params.shared_mac);
			}
		}

		// Compress and split packet
		let packet_id;
		let packets = if p_type == PacketType::Command
			|| p_type == PacketType::CommandLow
		{
			packet_id = None;
			algs::compress_and_split(self.is_client, packet)
		} else {
			// Set the inner packet id for voice packets
			if p_type == PacketType::Voice || p_type == PacketType::VoiceWhisper
			{
				(&mut packet.content_mut()[..2])
					.write_u16::<NetworkEndian>(con.outgoing_p_ids[type_i].1)
					.unwrap();
			}

			// Identify init packets by their number
			if p_type == PacketType::Init {
				if packet.direction() == Direction::S2C {
					packet_id = Some(u16::from(packet.content()[0]));
				} else {
					packet_id = Some(u16::from(packet.content()[4]));
				}
			} else {
				packet_id = None;
			}

			vec![packet]
		};

		let packets = packets
			.into_iter()
			.map(|mut packet| -> Result<_> {
				// Get packet id
				let (gen, mut p_id) = if p_type == PacketType::Init {
					(0, 0)
				} else {
					con.outgoing_p_ids[type_i]
				};
				let packet_id = if p_type != PacketType::Init {
					packet.packet_id(p_id);
					p_id
				} else {
					// Identify init packets by their number
					packet_id.unwrap()
				};

				// Encrypt if necessary
				if fake_encrypt {
					algs::encrypt_fake(&mut packet)?;
				} else if should_encrypt {
					// The params are set
					let params = con.params.as_mut().unwrap();
					algs::encrypt(
						&mut packet,
						gen,
						&params.shared_iv,
						&mut params.key_cache,
					)?;
				}

				// Increment outgoing_p_ids
				p_id = p_id.wrapping_add(1);
				let new_gen = if p_id == 0 { gen.wrapping_add(1) } else { gen };
				if p_type != PacketType::Init {
					con.outgoing_p_ids[type_i] = (new_gen, p_id);
				}
				Ok((gen, packet_id, packet.into_vec().into()))
			})
			.collect::<Result<Vec<_>>>()?;
		Ok(packets)
	}
}
