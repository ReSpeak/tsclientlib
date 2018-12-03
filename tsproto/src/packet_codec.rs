use std::borrow::Cow;
use std::io::Cursor;
use std::mem;
use std::net::SocketAddr;
use std::u16;

use bytes::Bytes;
use futures::sync::mpsc;
use futures::{future, Future, IntoFuture, Sink};
use num::ToPrimitive;
use slog::Logger;
use tokio;

use algorithms as algs;
use connection::Connection;
use connectionmanager::{ConnectionManager, Resender};
use handler_data::{ConnectionValue, Data, PacketObserver};
use packets::Data as PData;
use packets::*;
use {packets, Error, LockedHashMap, Result, MAX_FRAGMENTS_LENGTH, MAX_QUEUE_LEN};

/// Decodes incoming udp packets.
///
/// This part does the defragmentation, decryption and decompression.
pub struct PacketCodecReceiver<CM: ConnectionManager + 'static> {
	connections: LockedHashMap<CM::Key, ConnectionValue<CM::AssociatedData>>,
	is_client: bool,
	logger: Logger,
	in_packet_observer: LockedHashMap<String,
		Box<PacketObserver<CM::AssociatedData>>>,

	/// The sink for `UdpPacket`s with no known connection.
	///
	/// This can stay `None` so all packets without connection will be dropped.
	unknown_udp_packet_sink: Option<mpsc::Sender<(SocketAddr, Bytes)>>,
}

impl<CM: ConnectionManager + 'static> PacketCodecReceiver<CM> {
	pub fn new(
		data: &Data<CM>,
		unknown_udp_packet_sink: Option<mpsc::Sender<(SocketAddr, Bytes)>>,
	) -> Self {
		Self {
			connections: data.connections.clone(),
			is_client: data.is_client,
			logger: data.logger.clone(),
			in_packet_observer: data.in_packet_observer.clone(),
			unknown_udp_packet_sink,
		}
	}

	pub fn handle_udp_packet(
		&mut self,
		(addr, udp_packet): (SocketAddr, Bytes),
	) -> impl Future<Item = (), Error = Error> {
		// Parse header
		let (header, pos) = {
			let mut r = Cursor::new(&*udp_packet);
			(
				match packets::Header::read(&!self.is_client, &mut r) {
					Ok(r) => r,
					Err(e) => return future::err(e),
				},
				r.position() as usize,
			)
		};

		// Find the right connection
		let cons = self.connections.read().unwrap();
		if let Some(con) = cons.get(&CM::get_connection_key(addr, &header)).map(|vs| vs.clone())
		{
			// If we are a client and have only a single connection, we will do the
			// work inside this future and not spawn a new one.
			let logger = self.logger.new(o!("addr" => addr));
			let in_packet_observer = self.in_packet_observer.clone();
			if self.is_client && cons.len() == 1 {
				drop(cons);
				Self::connection_handle_udp_packet(
					&logger,
					in_packet_observer,
					self.is_client,
					&con,
					addr,
					&udp_packet,
					header,
					pos,
				).into_future()
			} else {
				drop(cons);
				let is_client = self.is_client;
				tokio::spawn(future::lazy(move || {
					if let Err(e) = Self::connection_handle_udp_packet(
						&logger,
						in_packet_observer,
						is_client,
						&con,
						addr,
						&udp_packet,
						header,
						pos,
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
				if sink.try_send((addr, udp_packet)).is_err() {
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
		in_packet_observer: LockedHashMap<String,
			Box<PacketObserver<CM::AssociatedData>>>,
		is_client: bool,
		connection: &ConnectionValue<CM::AssociatedData>,
		_: SocketAddr,
		udp_packet: &Bytes,
		header: packets::Header,
		pos: usize,
	) -> Result<()> {
		let con2 = connection.downgrade();
		let mut con = connection.mutex.lock().unwrap();
		let con = &mut *con;
		let dec_data;
		let dec_data1;
		let packet;
		let mut udp_packet = &udp_packet[pos..];
		let mut ack = None;
		let mut packets = None;

		let p_type = header.get_type();
		let type_i = p_type.to_usize().unwrap();
		let id = header.p_id;
		let (in_recv_win, gen_id, cur_next, limit) =
			con.1.in_receive_window(p_type, id);
		let r_queue = &mut con.1.receive_queue;
		let frag_queue = &mut con.1.fragmented_queue;
		let in_ids = &mut con.1.incoming_p_ids;

		if con.1.params.is_some() && p_type == PacketType::Init {
			return Err(Error::UnexpectedInitPacket);
		}

		// Ignore range for acks
		if p_type == PacketType::Ack || p_type == PacketType::AckLow
			|| in_recv_win
		{
			if !header.get_unencrypted() {
				// If it is the first ack packet of a client, try to fake
				// decrypt it.
				let decrypted = if (p_type == PacketType::Ack
					&& id <= 1
					&& is_client) || con.1.params.is_none() {
					if let Ok(dec) = algs::decrypt_fake(&header, udp_packet)
					{
						dec_data = dec;
						udp_packet = &dec_data;
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
							&header,
							udp_packet,
							gen_id,
							&params.shared_iv,
							&mut params.key_cache,
						);
						if dec_res.is_err() && p_type == PacketType::Ack
							&& id == 1 && is_client {
							// Ignore error, this is the ack packet for the
							// clientinit, we take the initserver as ack anyway.
							return Ok(());
						}

						dec_data1 = dec_res?;
						udp_packet = &dec_data1;
					} else {
						// Failed to fake decrypt the packet
						return Err(Error::WrongMac);
					}
				}
			} else if algs::must_encrypt(p_type) {
				// Check if it is ok for the packet to be unencrypted
				return Err(Error::UnallowedUnencryptedPacket);
			}
			match p_type {
				PacketType::Command | PacketType::CommandLow => {
					if p_type == PacketType::Command {
						ack = Some((
							PacketType::Ack,
							PData::Ack(id),
						));
					} else if p_type == PacketType::CommandLow {
						ack = Some((
							PacketType::AckLow,
							PData::AckLow(id),
						));
					}
					packets = Some(Self::handle_command_packet(
						logger, r_queue, frag_queue, in_ids, header,
						udp_packet,
					)?);

					// Dummy value
					packet = Err(Error::UnallowedUnencryptedPacket);
				}
				_ => {
					if p_type == PacketType::Ping {
						ack = Some((
							PacketType::Pong,
							PData::Pong(id),
						));
					}
					// Update packet ids
					let (id, next_gen) = id.overflowing_add(1);
					if p_type != PacketType::Init {
						in_ids[type_i] = (if next_gen { gen_id + 1 } else { gen_id }, id);
					}

					let p_data = PData::read(
						&header,
						&mut Cursor::new(&udp_packet),
					)?;
					match p_data {
						PData::Ack(p_id) | PData::AckLow(p_id) => {
							// Remove command packet from send queue if the fitting ack is received.
							let p_type =
								if p_type == PacketType::Ack {
									PacketType::Command
								} else {
									PacketType::CommandLow
								};
							con.1.resender.ack_packet(p_type, p_id);
							let mut p = Packet::new(header, p_data);
							for o in in_packet_observer.read().unwrap().values() {
								o.observe(con, &mut p);
							}
							return Ok(());
						}
						PData::VoiceS2C { .. }
						| PData::VoiceWhisperS2C { .. } => {
							// Seems to work better without assembling the first 3 voice packets
							// Use handle_voice_packet to assemble fragmented voice packets
							/*let mut res = Self::handle_voice_packet(&logger, params, &header, p_data);
							let res = res.drain(..).map(|p|
								(con_key.clone(), p)).collect();
							Ok(res)*/
							packet = Ok(Packet::new(header, p_data));
						}
						_ => packet = Ok(Packet::new(header, p_data)),
					}
				},
			}
		} else {
			// Send an ack for the case when it was lost
			if p_type == PacketType::Command {
				ack = Some((PacketType::Ack, PData::Ack(id)));
			} else if p_type == PacketType::CommandLow {
				ack = Some((PacketType::AckLow, PData::AckLow(id)));
			}
			packet = Err(Error::NotInReceiveWindow {
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
					.map_err(|_| ())
			);
		}

		if let Some(packets) = packets {
			// Be careful with command packets, they are
			// guaranteed to be in the right order now, because
			// we hold a lock on the connection.
			for mut p in packets {
				for o in in_packet_observer.read().unwrap().values() {
					o.observe(con, &mut p);
				}
				// Send to packet handler
				if let Err(e) = con.1.command_sink.unbounded_send(p) {
					error!(logger, "Failed to send command packet to \
					handler"; "error" => ?e);
				}
			}
			return Ok(());
		}

		let mut packet = packet?;
		for o in in_packet_observer.read().unwrap().values() {
			o.observe(con, &mut packet);
		}

		let sink = match packet.data {
			PData::VoiceS2C { .. } | PData::VoiceWhisperS2C { .. } => {
				&mut con.1.audio_sink
			}
			PData::Ping { .. } => return Ok(()),
			_ => {
				// Send as command packet
				&mut con.1.command_sink
			}
		};
		if let Err(e) = sink.unbounded_send(packet) {
			error!(logger, "Failed to send packet to handler"; "error" => ?e);
		}
		Ok(())
	}

	/// Handle `Command` and `CommandLow` packets.
	///
	/// They have to be handled in the right order.
	fn handle_command_packet(
		logger: &Logger,
		r_queue: &mut [Vec<(Header, Vec<u8>)>; 2],
		frag_queue: &mut [Option<(Header, Vec<u8>)>; 2],
		in_ids: &mut [(u32, u16); 8],
		mut header: Header,
		udp_packet: &[u8],
	) -> Result<Vec<Packet>> {
		let mut udp_packet = Cow::Borrowed(udp_packet);
		let mut id = header.p_id;
		let type_i = header.get_type().to_usize().unwrap();
		let cmd_i = if header.get_type() == PacketType::Command {
			0
		} else {
			1
		};
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

				let res_packet = if header.get_fragmented() {
					if let Some((header, mut frag_queue)) = frag_queue.take() {
						// Last fragmented packet
						frag_queue.extend_from_slice(&udp_packet);
						// Decompress
						let decompressed = if header.get_compressed() {
							//debug!(logger, "Compressed"; "data" => ?::HexSlice(&frag_queue));
							::quicklz::decompress(
								&mut Cursor::new(frag_queue),
								::MAX_DECOMPRESSED_SIZE,
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
						let p_data = PData::read(
							&header,
							&mut Cursor::new(decompressed.as_slice()),
						)?;
						Some(Packet::new(header, p_data))
					} else {
						// Enqueue
						*frag_queue = Some((
							header,
							mem::replace(&mut udp_packet, Cow::Borrowed(&[]))
								.into_owned(),
						));
						None
					}
				} else if let Some((_, ref mut frag_queue)) = *frag_queue {
					// The packet is fragmented
					if frag_queue.len() < MAX_FRAGMENTS_LENGTH {
						frag_queue.extend_from_slice(&udp_packet);
						None
					} else {
						return Err(Error::MaxLengthExceeded(String::from(
							"fragment queue",
						)));
					}
				} else {
					// Decompress
					let decompressed = if header.get_compressed() {
						//debug!(logger, "Compressed"; "data" => ?::HexSlice(&packet.0));
						::quicklz::decompress(
							&mut Cursor::new(udp_packet),
							::MAX_DECOMPRESSED_SIZE,
						)?
					} else {
						mem::replace(&mut udp_packet, Cow::Borrowed(&[]))
							.into_owned()
					};
					/*if header.get_compressed() {
						debug!(logger, "Decompressed"; "data" => ?::HexSlice(&decompressed));
					}*/
					let p_data = PData::read(
						&header,
						&mut Cursor::new(decompressed.as_slice()),
					)?;
					Some(Packet::new(header, p_data))
				};
				if let Some(p) = res_packet {
					packets.push(p);
				}

				// Check if there are following packets in the receive queue.
				id = id.wrapping_add(1);
				if let Some(pos) = r_queue
					.iter()
					.position(|&(ref header, _)| header.p_id == id)
				{
					let (h, p) = r_queue.remove(pos);
					header = h;
					udp_packet = Cow::Owned(p);
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
				r_queue.push((header, udp_packet.to_vec()));
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
	pub fn new(is_client: bool) -> Self {
		Self { is_client }
	}

	pub fn encode_packet(
		&self,
		con: &mut Connection,
		mut packet: Packet,
	) -> Result<Vec<(u16, Bytes)>> {
		let p_type = packet.header.get_type();
		let type_i = p_type.to_usize().unwrap();

		let use_newprotocol = (p_type == PacketType::Command
			|| p_type == PacketType::CommandLow)
			&& self.is_client;

		// Change state on disconnect
		if let PData::Command(cmd) = &packet.data {
			if cmd.command == "clientdisconnect" {
				con.resender.handle_event(
					::connectionmanager::ResenderEvent::Disconnecting,
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
			&& gen == 0 && ((!self.is_client && p_id == 0)
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
			should_encrypt = algs::should_encrypt(
				p_type,
				params.voice_encryption,
			);
			c_id = params.c_id;
		} else {
			should_encrypt = algs::should_encrypt(
				p_type,
				false,
			);
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
			}).collect::<Result<Vec<_>>>()?;
		Ok(packets)
	}
}
