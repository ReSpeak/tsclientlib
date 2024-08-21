use std::io::Cursor;
use std::task::Context;

use num_traits::ToPrimitive;
use omnom::WriteExt;
use tracing::warn;
use tsproto_packets::packets::*;

use crate::algorithms as algs;
use crate::connection::{Connection, Event, StreamItem};
use crate::resend::{PartialPacketId, Resender};
use crate::{Error, Result, MAX_FRAGMENTS_LENGTH, MAX_QUEUE_LEN};

/// Encodes outgoing packets.
///
/// This part does the compression, encryption and fragmentation.
#[derive(Clone, Debug, Default)]
pub struct PacketCodec {
	/// The next packet id that should be sent.
	///
	/// This list is indexed by the `PacketType`, `PacketType::Init` is an
	/// invalid index.
	pub outgoing_p_ids: [PartialPacketId; 8],
	/// Used for incoming out-of-order packets.
	///
	/// Only used for `Command` and `CommandLow` packets.
	pub receive_queue: [Vec<Vec<u8>>; 2],
	/// Used for incoming fragmented packets.
	///
	/// Contains the accumulated data from fragmented packets, the header of the
	/// first packet is at the beginning, all other headers are stripped away.
	/// Only used for `Command` and `CommandLow` packets.
	pub fragmented_queue: [Option<Vec<u8>>; 2],
	/// The next packet id that is expected.
	///
	/// Works like the `outgoing_p_ids`.
	pub incoming_p_ids: [PartialPacketId; 8],
}

impl PacketCodec {
	/// Handle a packet for a specific connection.
	///
	/// This part does the defragmentation, decryption and decompression.
	pub fn handle_udp_packet(
		con: &mut Connection, cx: &mut Context, mut packet_data: Vec<u8>,
	) -> Result<()> {
		let mut ack = false;

		let dir = if con.is_client { Direction::S2C } else { Direction::C2S };
		let packet = InPacket::new(dir, &packet_data);
		let p_type = packet.header().packet_type();
		let type_i = p_type.to_usize().unwrap();
		let id = packet.header().packet_id();
		let (in_recv_win, gen_id, cur_next, limit) = con.in_receive_window(p_type, id);

		con.resender.handle_loss_incoming(&packet, in_recv_win, cur_next);

		if let Some(params) = &con.params {
			if p_type == PacketType::Init {
				con.stream_items.push_back(StreamItem::Error(Error::UnexpectedInitPacket));
				return Ok(());
			}
			if !con.is_client {
				let c_id = packet.header().client_id().unwrap();
				// Accept any client id for the first few acks
				let is_first_ack = gen_id == 0 && id <= 3 && p_type == PacketType::Ack;
				if c_id != params.c_id && !is_first_ack {
					con.stream_items.push_back(StreamItem::Error(Error::WrongClientId(c_id)));
					return Ok(());
				}
			}
		}

		// Ignore range for acks and audio packets
		if [PacketType::Ack, PacketType::AckLow, PacketType::Voice, PacketType::VoiceWhisper]
			.contains(&p_type)
			|| in_recv_win
		{
			if !packet.header().flags().contains(Flags::UNENCRYPTED) {
				if p_type == PacketType::Ack && id == 1 && con.is_client {
					// This is the ack packet for the clientinit, we take the
					// initserver as ack instead.
					return Ok(());
				}

				// If it is the first ack packet of a client, try to fake
				// decrypt it.
				let new_content = if (p_type == PacketType::Ack && id <= 1 && con.is_client)
					|| con.params.is_none()
				{
					match algs::decrypt_fake(&packet).or_else(|_| {
						if let Some(params) = &mut con.params {
							// Decrypt the packet
							algs::decrypt(&packet, gen_id, &params.shared_iv, &mut params.key_cache)
						} else {
							// Failed to fake decrypt the packet
							Err(Error::WrongMac { p_type, generation_id: gen_id, packet_id: id })
						}
					}) {
						Ok(r) => r,
						Err(e) => {
							con.stream_items.push_back(StreamItem::Error(e));
							return Ok(());
						}
					}
				} else if let Some(params) = &mut con.params {
					// Decrypt the packet
					match algs::decrypt(&packet, gen_id, &params.shared_iv, &mut params.key_cache) {
						Ok(r) => r,
						Err(e) => {
							con.stream_items.push_back(StreamItem::Error(e));
							return Ok(());
						}
					}
				} else {
					// Failed to fake decrypt the packet
					con.stream_items.push_back(StreamItem::Error(Error::WrongMac {
						p_type,
						generation_id: gen_id,
						packet_id: id,
					}));
					return Ok(());
				};

				let start = packet.header().data().len();
				packet_data[start..].copy_from_slice(&new_content);
			} else if algs::must_encrypt(p_type) {
				// Check if it is ok for the packet to be unencrypted
				con.stream_items.push_back(StreamItem::Error(Error::UnallowedUnencryptedPacket));
				return Ok(());
			}

			let packet = InPacket::new(dir, &packet_data);
			match p_type {
				PacketType::Command | PacketType::CommandLow => {
					ack = true;

					let commands = Self::handle_command_packet(con, packet_data)?;

					// Be careful with command packets, they are guaranteed to
					// be in the right order now.
					for c in commands {
						// Send again
						let packet = InPacket::new(dir, &c);
						let event = Event::ReceivePacket(&packet);
						con.send_event(&event);

						let item = match InCommandBuf::try_new(dir, c) {
							Ok(c) => {
								// initivexpand2 is the ack for the last init packet
								if con.is_client
									&& c.data().packet().content().starts_with(b"initivexpand2 ")
								{
									Resender::ack_packet(con, cx, PacketType::Init, 4);
								} else if con.is_client
									&& c.data().packet().content().starts_with(b"initserver ")
								{
									// initserver acks clientinit
									Resender::ack_packet(con, cx, PacketType::Command, 2);
								} else if !con.is_client
									&& c.data().packet().content().starts_with(b"clientek ")
								{
									// clientek acks initivexpand2
									Resender::ack_packet(con, cx, PacketType::Command, 0);
								}
								StreamItem::Command(c)
							}
							Err(e) => return Err(Error::PacketParse("command", e)),
						};
						con.stream_items.push_back(item);
					}
				}
				_ => {
					if p_type == PacketType::Ping {
						ack = true;
					}
					// Update packet ids
					let in_ids = &mut con.codec.incoming_p_ids;
					if p_type != PacketType::Init {
						let (id, next_gen) = id.overflowing_add(1);
						let generation_id = if next_gen { gen_id + 1 } else { gen_id };
						in_ids[type_i] = PartialPacketId { generation_id, packet_id: id };
					}

					match packet.ack_packet() {
						Ok(Some(ack_id)) => {
							// Remove command packet from send queue if the fitting ack is received.
							let p_type = if p_type == PacketType::Ack {
								PacketType::Command
							} else if p_type == PacketType::AckLow {
								PacketType::CommandLow
							} else if p_type == PacketType::Pong {
								PacketType::Ping
							} else {
								p_type
							};
							Resender::ack_packet(con, cx, p_type, ack_id);
						}
						Ok(None) => {}
						Err(e) => {
							con.stream_items.push_back(StreamItem::Error(Error::CreateAck(e)));
							return Ok(());
						}
					}

					// Send event after handling acks
					let event = Event::ReceivePacket(&packet);
					con.send_event(&event);

					if p_type.is_voice() {
						con.stream_items.push_back(match InAudioBuf::try_new(dir, packet_data) {
							Ok(r) => StreamItem::Audio(r),
							Err(e) => StreamItem::Error(Error::PacketParse("audio", e)),
						});
					} else if p_type == PacketType::Init {
						con.stream_items.push_back(if con.is_client {
							match InS2CInitBuf::try_new(dir, packet_data) {
								Ok(r) => StreamItem::S2CInit(r),
								Err(e) => StreamItem::Error(Error::PacketParse("s2cinit", e)),
							}
						} else {
							match InC2SInitBuf::try_new(dir, packet_data) {
								Ok(r) => StreamItem::C2SInit(r),
								Err(e) => StreamItem::Error(Error::PacketParse("c2sinit", e)),
							}
						});
					}
				}
			}
		} else {
			// Send an ack for the case when it was lost
			if p_type == PacketType::Command || p_type == PacketType::CommandLow {
				ack = true;
				con.resender.handle_loss_resend_ack();
			}
			con.stream_items.push_back(StreamItem::Error(Error::NotInReceiveWindow {
				id,
				next: cur_next,
				limit,
				p_type,
			}));
		}

		// Send ack
		if ack {
			con.send_ack_packet(cx, OutAck::new(dir.reverse(), p_type, id))?;
		}

		Ok(())
	}

	/// Handle `Command` and `CommandLow` packets.
	///
	/// They have to be handled in the right order.
	fn handle_command_packet(
		con: &mut Connection, mut packet_data: Vec<u8>,
	) -> Result<Vec<Vec<u8>>> {
		let dir = if con.is_client { Direction::S2C } else { Direction::C2S };
		let mut packet = InPacket::new(dir, &packet_data);
		let header = packet.header();
		let p_type = header.packet_type();
		let mut id = header.packet_id();
		let type_i = p_type.to_usize().unwrap();
		let cmd_i = if p_type == PacketType::Command { 0 } else { 1 };
		let r_queue = &mut con.codec.receive_queue[cmd_i];
		let frag_queue = &mut con.codec.fragmented_queue[cmd_i];
		let in_ids = &mut con.codec.incoming_p_ids[type_i];
		let cur_next = in_ids.packet_id;
		if cur_next == id {
			// In order
			let mut packets = Vec::new();
			loop {
				// Update next packet id
				*in_ids = *in_ids + 1;

				let flags = packet.header().flags();
				let res_packet = if flags.contains(Flags::FRAGMENTED) {
					if let Some(mut frag_queue) = frag_queue.take() {
						// Last fragmented packet
						frag_queue.extend_from_slice(packet.content());
						let header = InPacket::new(dir, &frag_queue);
						// Decompress
						if header.header().flags().contains(Flags::COMPRESSED) {
							let decompressed = quicklz::decompress(
								&mut Cursor::new(header.content()),
								crate::MAX_DECOMPRESSED_SIZE,
							)
							.map_err(Error::DecompressPacket)?;
							let start = header.header().data().len();
							frag_queue.truncate(start);
							frag_queue.extend_from_slice(&decompressed);
						}

						Some(frag_queue)
					} else {
						// Enqueue
						*frag_queue = Some(packet_data);
						None
					}
				} else if let Some(frag_queue) = frag_queue {
					// The packet is fragmented
					if frag_queue.len() < MAX_FRAGMENTS_LENGTH {
						frag_queue.extend_from_slice(packet.content());
						None
					} else {
						return Err(Error::MaxLengthExceeded("fragment queue"));
					}
				} else {
					// Decompress
					if flags.contains(Flags::COMPRESSED) {
						let decompressed = quicklz::decompress(
							&mut Cursor::new(packet.content()),
							crate::MAX_DECOMPRESSED_SIZE,
						)
						.map_err(Error::DecompressPacket)?;
						let start = packet.header().data().len();
						packet_data.truncate(start);
						packet_data.extend_from_slice(&decompressed);
					};
					Some(packet_data)
				};
				if let Some(p) = res_packet {
					packets.push(p);
				}

				// Check if there are following packets in the receive queue.
				id = id.wrapping_add(1);
				if let Some(pos) =
					r_queue.iter().position(|p| InHeader::new(dir, p).packet_id() == id)
				{
					packet_data = r_queue.remove(pos);
					packet = InPacket::new(dir, &packet_data);
				} else {
					break;
				}
			}

			Ok(packets)
		} else {
			// Out of order
			warn!(got = id, expected = cur_next, "Out of order command packet");
			let (limit, next_gen) = cur_next.overflowing_add(MAX_QUEUE_LEN);
			if (!next_gen && id >= cur_next && id < limit)
				|| (next_gen && (id >= cur_next || id < limit))
			{
				r_queue.push(packet_data);
				Ok(vec![])
			} else {
				Err(Error::MaxLengthExceeded("command queue"))
			}
		}
	}

	pub fn encode_packet(con: &mut Connection, mut packet: OutPacket) -> Result<Vec<OutUdpPacket>> {
		let p_type = packet.header().packet_type();
		let type_i = p_type.to_usize().unwrap();

		// TODO Needed, commands should set their own flag?
		if (p_type == PacketType::Command || p_type == PacketType::CommandLow) && con.is_client {
			// Set newprotocol flag
			packet.flags(packet.header().flags() | Flags::NEWPROTOCOL);
		}

		let p_id = if p_type == PacketType::Init {
			PartialPacketId { generation_id: 0, packet_id: 0 }
		} else {
			con.codec.outgoing_p_ids[type_i]
		};
		// We fake encrypt the first command packet of the server (id 0) and the
		// first command packet of the client (id 1, clientek).
		let mut fake_encrypt = p_type == PacketType::Command
			&& p_id.generation_id == 0
			&& ((!con.is_client && p_id.packet_id == 0)
				|| (con.is_client && p_id.packet_id == 1 && {
					// Test if it is a clientek packet
					packet.content().starts_with(b"clientek")
				}));
		// Also fake encrypt the first ack of the client, which is the response
		// for the initivexpand2 packet.
		fake_encrypt |= con.is_client
			&& p_type == PacketType::Ack
			&& p_id.generation_id == 0
			&& p_id.packet_id == 0;

		// Get values from parameters
		let should_encrypt;
		let c_id;
		if let Some(params) = con.params.as_mut() {
			should_encrypt = algs::should_encrypt(p_type, params.voice_encryption);
			c_id = params.c_id;
		} else {
			should_encrypt = algs::should_encrypt(p_type, false);
			if should_encrypt {
				fake_encrypt = true;
			}
			c_id = 0;
		}

		// Client id for clients
		if con.is_client {
			packet.client_id(c_id);
		}

		if !should_encrypt && !fake_encrypt {
			packet.flags(packet.header().flags() | Flags::UNENCRYPTED);
			if let Some(params) = con.params.as_mut() {
				packet.mac().copy_from_slice(&params.shared_mac);
			}
		}

		// Compress and split packet
		let packets = if p_type == PacketType::Command || p_type == PacketType::CommandLow {
			algs::compress_and_split(con.is_client, packet)
		} else {
			// Set the inner packet id for voice packets
			if con.is_client && (p_type == PacketType::Voice || p_type == PacketType::VoiceWhisper)
			{
				(&mut packet.content_mut()[..2])
					.write_be(con.codec.outgoing_p_ids[type_i].packet_id)
					.unwrap();
			}

			vec![packet]
		};

		let packets = packets
			.into_iter()
			.map(|mut packet| -> Result<_> {
				// Get packet id
				let p_id = if p_type == PacketType::Init {
					PartialPacketId { generation_id: 0, packet_id: 0 }
				} else {
					packet.packet_id(con.codec.outgoing_p_ids[type_i].packet_id);
					con.codec.outgoing_p_ids[type_i]
				};

				// Encrypt if necessary
				if fake_encrypt {
					algs::encrypt_fake(&mut packet)?;
				} else if should_encrypt {
					// The params are set
					let params = con.params.as_mut().unwrap();
					algs::encrypt(
						&mut packet,
						p_id.generation_id,
						&params.shared_iv,
						&mut params.key_cache,
					)?;
				}

				// Increment outgoing_p_ids
				let p_id = p_id + 1;
				if p_type != PacketType::Init {
					con.codec.outgoing_p_ids[type_i] = p_id;
				}
				Ok(OutUdpPacket::new(p_id.generation_id, packet))
			})
			.collect::<Result<Vec<_>>>()?;
		Ok(packets)
	}
}
