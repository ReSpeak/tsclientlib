use std::borrow::Cow;
use std::io::Cursor;
use std::mem;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::u16;

use {evmap, tokio};
use bytes::{Bytes, BytesMut};
use futures::{future, Future, IntoFuture, Sink};
use futures::sync::mpsc;
use num::ToPrimitive;
use slog::Logger;

use {packets, Error, Result, MAX_FRAGMENTS_LENGTH, MAX_QUEUE_LEN };
use algorithms as algs;
use connection::{ConnectedParams, Connection};
use connectionmanager::{ConnectionManager, Resender};
use handler_data::{ConnectionValue, Data};
use packets::*;
use packets::Data as PData;

/// Decodes incoming udp packets.
///
/// This part does the defragmentation, decryption and decompression.
pub struct PacketCodecReceiver<CM: ConnectionManager + 'static> {
    connections: evmap::ReadHandle<CM::Key, ConnectionValue<CM::AssociatedData>>,
    is_client: bool,
    logger: Logger,
    log_packets: Arc<AtomicBool>,

    /// The sink for `UdpPacket`s with no known connection.
    ///
    /// This can stay `None` so all packets without connection will be dropped.
    unknown_udp_packet_sink: Option<mpsc::Sender<(SocketAddr, BytesMut)>>,
}

impl<CM: ConnectionManager + 'static>
    PacketCodecReceiver<CM> {
    pub fn new(
        data: &Data<CM>,
        unknown_udp_packet_sink: Option<mpsc::Sender<(SocketAddr, BytesMut)>>,
    ) -> Self {
        Self {
            connections: data.connections.clone(),
            is_client: data.is_client,
            logger: data.logger.clone(),
            log_packets: data.log_config.log_packets.clone(),
            unknown_udp_packet_sink,
        }
    }

    pub fn handle_udp_packet(
        &mut self,
        (addr, udp_packet): (SocketAddr, BytesMut),
    ) -> impl Future<Item=(), Error=Error> {
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
        if let Some(con) = self.connections.get_and(&CM::get_connection_key(addr, &header),
            |vs| vs[0].clone()) {
            // If we are a client and have only a single connection, we will do the
            // work inside this future and not spawn a new one.
            let logger = self.logger.new(o!("addr" => addr));
            let log_packets = self.log_packets.load(Ordering::Relaxed);
            if self.is_client && self.connections.len() == 1 {
                Self::connection_handle_udp_packet(
                    &logger, log_packets, self.is_client,
                    &con, addr, &udp_packet, header, pos).into_future()
            } else {
                let is_client = self.is_client;
                tokio::spawn(future::lazy(move || {
                    if let Err(e) = Self::connection_handle_udp_packet(
                        &logger, log_packets, is_client,
                        &con, addr, &udp_packet, header, pos) {
                        error!(logger, "Error handling udp packed"; "error" => ?e);
                    }
                    Ok(())
                }));
                future::ok(())
            }
        } else {
            // Unknown connection
            if let Some(sink) = &mut self.unknown_udp_packet_sink {
                // Don't block if the queue is full
                if sink.try_send((addr, udp_packet)).is_err() {
                    warn!(self.logger, "Unknown connection handler overloaded \
                        – dropping udp packet");
                }
            } else {
                warn!(self.logger, "Dropped packet without connection because \
                    no unknown packet handler is set");
            }
            future::ok(())
        }
    }

    /// Handle a packet for a specific connection.
    ///
    /// This part does the defragmentation, decryption and decompression.
    pub fn connection_handle_udp_packet(
        logger: &Logger,
        log_packets: bool,
        is_client: bool,
        connection: &ConnectionValue<CM::AssociatedData>,
        _: SocketAddr,
        udp_packet: &BytesMut,
        header: packets::Header,
        pos: usize,
    ) -> Result<()> {
        let con2 = connection.downgrade();
        let mut con = connection.mutex.lock().unwrap();
        let con = &mut *con;
        let dec_data;
        let dec_data1;
        let mut udp_packet = &udp_packet[pos..];
        let mut ack = None;
        let mut packets = None;
        let packet = if let Some(ref mut params) = con.1.params {
            if header.get_type() == PacketType::Init {
                return Err(Error::UnexpectedInitPacket);
            }
            let p_type = header.get_type();
            let type_i = p_type.to_usize().unwrap();
            let id = header.p_id;

            let (in_recv_win, gen_id, cur_next, limit) =
                params.in_receive_window(p_type, id);
            // Ignore range for acks
            let res = if p_type == PacketType::Ack
                || p_type == PacketType::AckLow
                || in_recv_win
            {
                if !header.get_unencrypted() {
                    // If it is the first ack packet of a
                    // client, try to fake decrypt it.
                    let decrypted = if header.get_type() == PacketType::Ack
                        && header.p_id == 0 && is_client
                    {
                        if let Ok(dec) = algs::decrypt_fake(
                            &header,
                            udp_packet,
                        ) {
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
                        // Decrypt the packet
                        dec_data1 = algs::decrypt(
                            &header,
                            udp_packet,
                            gen_id,
                            &params.shared_iv,
                            &mut params.key_cache,
                        )?;
                        udp_packet = &dec_data1;
                    }
                } else if algs::must_encrypt(header.get_type()) {
                    // Check if it is ok for the packet to be unencrypted
                    return Err(Error::UnallowedUnencryptedPacket);
                }
                match header.get_type() {
                    PacketType::Command |
                    PacketType::CommandLow => {
                        if header.get_type()
                            == PacketType::Command
                        {
                            ack = Some((
                                PacketType::Ack,
                                PData::Ack(header.p_id),
                            ));
                        } else if header.get_type()
                            == PacketType::CommandLow
                        {
                            ack = Some((
                                PacketType::AckLow,
                                PData::AckLow(
                                    header.p_id,
                                ),
                            ));
                        }
                        packets = Some(Self::handle_command_packet(
                            logger,
                            params,
                            header,
                            udp_packet,
                        )?);

                        Err(Error::UnallowedUnencryptedPacket)
                    }
                    _ => Ok({
                        if header.get_type() == PacketType::Ping {
                            ack = Some((
                                PacketType::Pong,
                                PData::Pong(
                                    header.p_id,
                                ),
                            ));
                        }
                        // Update packet ids
                        let id = id.wrapping_add(1);
                        params.incoming_p_ids[type_i] = (gen_id, id);

                        let p_data = PData::read(
                            &header,
                            &mut Cursor::new(&udp_packet),
                        )?;
                        match p_data {
                            PData::Ack(p_id) |
                            PData::AckLow(p_id) => {
                                // Remove command packet from send queue if the fitting ack is received.
                                let p_type = if header.get_type()
                                    == PacketType::Ack {
                                    PacketType::Command
                                } else {
                                    PacketType::CommandLow
                                };
                                con.1.resender.ack_packet(p_type, p_id);
                                return Ok(());
                            }
                            PData::VoiceS2C { .. } |
                            PData::VoiceWhisperS2C { .. } => {
                                // Seems to work better without assembling the first 3 voice packets
                                // Use handle_voice_packet to assemble fragmented voice packets
                                /*let mut res = Self::handle_voice_packet(&logger, params, &header, p_data);
                                let res = res.drain(..).map(|p|
                                    (con_key.clone(), p)).collect();
                                Ok(res)*/
                                Packet::new(header, p_data)
                            }
                            _ => Packet::new(header, p_data),
                        }
                    })
                }
            } else {
                // Send an ack for the case when it was lost
                if header.get_type() == PacketType::Command {
                    ack = Some((
                        PacketType::Ack,
                        PData::Ack(header.p_id),
                    ));
                } else if header.get_type() == PacketType::CommandLow {
                    ack = Some((
                        PacketType::AckLow,
                        PData::AckLow(header.p_id),
                    ));
                }
                Err(Error::NotInReceiveWindow {
                    id,
                    next: cur_next,
                    limit,
                    p_type: header.get_type(),
                })
            };
            res
        } else {
            // Try to fake decrypt the packet
            if let Ok(dec) = algs::decrypt_fake(&header, &udp_packet) {
                dec_data = dec;
                udp_packet = &dec_data;
                // Send ack
                if header.get_type() == PacketType::Command {
                    ack = Some((
                        PacketType::Ack,
                        PData::Ack(header.p_id),
                    ));
                }
            }
            let p_data = PData::read(
                &header,
                &mut Cursor::new(&*udp_packet),
            )?;
            Ok(Packet::new(header, p_data))
        };

        // Send ack
        if let Some((ack_type, ack_packet)) = ack {
            let mut ack_header = Header::default();
            ack_header.set_type(ack_type);
            let logger = logger.clone();
            tokio::spawn(con2.as_packet_sink().send(Packet::new(ack_header, ack_packet))
                .map(|_| ())
                .map_err(move |e| {
                    error!(logger, "Failed to send ack packet"; "error" => ?e);
                }));
        }

        if let Some(packets) = packets {
            // Be careful with command packets, they are
            // guaranteed to be in the right order now, because
            // we hold a lock on the connection.
            for p in packets {
                if log_packets {
                    ::log::PacketLogger::log_packet(&logger, is_client, true,
                        &p);
                }
                // Send to packet handler
                if let Err(e) = con.1.command_sink.unbounded_send(p) {
                    error!(logger, "Failed to send command packet to \
                        handler"; "error" => ?e);
                }
            }
            return Ok(());
        }

        let packet = packet?;
        if log_packets {
            ::log::PacketLogger::log_packet(&logger, is_client, true, &packet);
        }

        let sink = match packet.data {
            PData::VoiceS2C { .. } |
            PData::VoiceWhisperS2C { .. } =>
                &mut con.1.audio_sink,
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
        params: &mut ConnectedParams,
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
        let r_queue = &mut params.receive_queue[cmd_i];
        let frag_queue = &mut params.fragmented_queue[cmd_i];
        let in_ids = &mut params.incoming_p_ids[type_i];
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
                        *frag_queue = Some((header,
                            mem::replace(&mut udp_packet, Cow::Borrowed(&[]))
                            .into_owned()));
                        None
                    }
                } else if let Some((_, ref mut frag_queue)) = *frag_queue {
                    // The packet is fragmented
                    if frag_queue.len() < MAX_FRAGMENTS_LENGTH {
                        frag_queue.extend_from_slice(&udp_packet);
                        None
                    } else {
                        return Err(Error::MaxLengthExceeded(String::from(
                            "fragment queue")));
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
    logger: Logger,
}

impl PacketCodecSender {
    pub fn new(is_client: bool, logger: Logger) -> Self {
        Self { is_client, logger }
    }

    pub fn encode_packet(&self, con: &mut Connection, mut packet: Packet)
        -> Result<Vec<(u16, Bytes)>> {
        let use_newprotocol =
            (packet.header.get_type() == PacketType::Command
            || packet.header.get_type() == PacketType::CommandLow)
            && self.is_client;

        // Change state on disconnect
        if let PData::Command(cmd) = &packet.data {
            if cmd.command == "clientdisconnect" {
                con.resender.handle_event(
                    ::connectionmanager::ResenderEvent::Disconnecting);
            }
        }

        // Get the connection parameters
        if let Some(params) = con.params.as_mut() {
            let p_type = packet.header.get_type();
            let type_i = p_type.to_usize().unwrap();

            let (gen, p_id) = params.outgoing_p_ids[type_i];
            // We fake encrypt the first command packet of the
            // server (id 0) and the first command packet of the
            // client (id 1) if the client uses the new protocol
            // (the packet is a clientek).
            let fake_encrypt =
                p_type == PacketType::Command && gen == 0
                && ((!self.is_client && p_id == 0)
                    || (self.is_client && p_id == 1 && {
                        // Test if it is a clientek packet
                        if let PData::Command(ref cmd) =
                            packet.data {
                            cmd.command == "clientek"
                        } else {
                            false
                        }
                    }));

            // Compress and split packet
            let mut packets = if p_type == PacketType::Command
                || p_type == PacketType::CommandLow
            {
                algs::compress_and_split(self.is_client, &packet)
            } else {
                // Set the inner packet id for voice packets
                match packet.data {
                    PData::VoiceC2S { ref mut id, .. }           |
                    PData::VoiceS2C { ref mut id, .. }           |
                    PData::VoiceWhisperC2S { ref mut id, .. }    |
                    PData::VoiceWhisperNewC2S { ref mut id, .. } |
                    PData::VoiceWhisperS2C { ref mut id, .. } => {
                        *id = params.outgoing_p_ids[type_i].1;
                    }
                    _ => {}
                }

                let mut data = Vec::new();
                packet.data.write(&mut data).unwrap();
                vec![(packet.header, data)]
            };
            let packets = packets
                .drain(..)
                .map(|(mut header, mut p_data)| -> Result<_> {
                    // Packet data (without header)
                    // Get packet id
                    let (mut gen, mut p_id) = params.outgoing_p_ids[type_i];
                    header.p_id = p_id;

                    // Client id for clients
                    if self.is_client {
                        header.c_id = Some(params.c_id);
                    } else {
                        header.c_id = None;
                    }

                    // Set newprotocol flag if needed
                    if use_newprotocol {
                        header.set_newprotocol(true);
                    }

                    // Encrypt if necessary, fake encrypt the initivexpand packet
                    if algs::should_encrypt(
                        header.get_type(),
                        params.voice_encryption,
                    ) {
                        header.set_unencrypted(false);
                        if fake_encrypt {
                            p_data = algs::encrypt_fake(&mut header, &p_data)?;
                        } else {
                            p_data = algs::encrypt(
                                &mut header,
                                &p_data,
                                gen,
                                &params.shared_iv,
                                &mut params.key_cache,
                            )?;
                        }
                    } else {
                        header.set_unencrypted(true);
                        header.mac.copy_from_slice(&params.shared_mac);
                    };

                    // Increment outgoing_p_ids
                    p_id = p_id.wrapping_add(1);
                    if p_id == 0 {
                        gen = gen.wrapping_add(1);
                    }
                    params.outgoing_p_ids[type_i] = (gen, p_id);
                    let mut buf = Vec::new();
                    header.write(&mut buf)?;
                    buf.append(&mut p_data);
                    Ok((header.p_id, buf.into()))
                })
                .collect::<Result<Vec<_>>>()?;
            Ok(packets)
        } else {
            // No connection params available
            let mut p_data = Vec::new();
            packet.data.write(&mut p_data).unwrap();
            let mut header = packet.header;
            // Client id for clients
            if self.is_client {
                header.c_id = Some(0);
            } else {
                header.c_id = None;
            }
            // Fake encrypt if needed
            if algs::should_encrypt(header.get_type(), false) {
                header.set_unencrypted(false);
                p_data = algs::encrypt_fake(&mut header, &p_data)?;
            }

            // Identify init packets by their number
            let p_id = match packet.data {
                PData::C2SInit(C2SInit::Init0 { .. }) => 0,
                PData::C2SInit(C2SInit::Init2 { .. }) => 2,
                PData::C2SInit(C2SInit::Init4 { .. }) => 4,
                PData::S2CInit(S2CInit::Init1 { .. }) => 1,
                PData::S2CInit(S2CInit::Init3 { .. }) => 3,
                _ => header.p_id,
            };

            let mut buf = Vec::new();
            header.write(&mut buf)?;
            buf.append(&mut p_data);
            Ok(vec![(p_id, buf.into())])
        }
    }
}
