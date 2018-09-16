use std::cell::RefCell;
use std::io::Cursor;
use std::net::SocketAddr;
use std::rc::{Rc, Weak};
use std::u16;

use {evmap, futures_locks, tokio};
use futures::{self, future, Future, Sink, Stream, task};
use futures::sync::mpsc;
use num::ToPrimitive;
use slog::Logger;

use {packets, Error, Result, MAX_FRAGMENTS_LENGTH, MAX_QUEUE_LEN };
use algorithms as algs;
use connection::{ConnectedParams, Connection};
use connectionmanager::{ConnectionManager, Resender};
use handler_data::{ConnectionValue, Data};
use packets::*;

/// Decodes incoming udp packets.
///
/// This part does the defragmentation, decryption and decompression.
pub struct PacketCodecReceiver<CM: ConnectionManager + 'static> {
    connections: evmap::ReadHandle<CM::Key, ConnectionValue<CM>>,
    is_client: bool,
    logger: Logger,

    /// The sink for `UdpPacket`s with no known connection.
    ///
    /// This can stay `None` so all packets without connection will be dropped.
    pub unknown_udp_packet_sink:
        Option<mpsc::Sender<(SocketAddr, UdpPacket)>>,
}

impl<CM: ConnectionManager + 'static> PacketCodecReceiver<CM> {
    pub fn new(data: &Data<CM>) -> Self {
        Self {
            connections: data.connections.clone(),
            is_client: data.is_client,
            logger: data.logger.clone(),
            receive_buffer: Vec::new(),
        }
    }

    pub fn handle_udp_packet(
        &mut self,
        (addr, udp_packet): (SocketAddr, UdpPacket),
    ) -> Box<Future<Item=(), Error=Error>> {
        // Parse header
        let (header, pos) = {
            let mut r = Cursor::new(&udp_packet.0);
            (
                match packets::Header::read(&!self.is_client, &mut r) {
                    Ok(r) => r,
                    Err(e) => return Box::new(future::err(e)),
                },
                r.position() as usize,
            )
        };

        // Find the right connection
        if let Some(con) = self.connections.get_and(&CM::get_connection_key(addr, &header),
            |vs| vs[0].mutex.clone()) {
            // If we are a client and have only a single connection, we will do the
            // work inside this future and not spawn a new one.
            let logger = self.logger.clone();
            if self.is_client && self.connections.len() == 1 {
                Box::new(Self::connection_handle_udp_packet(self.logger.clone(), con, addr, udp_packet, header, pos)
                    .then(|r| {
                        if let Err(e) = r {
                            error!(logger, "Error handling udp packed"; "error" => ?e);
                        }
                        Ok(())
                    }))
            } else {
                tokio::spawn(move ||
                    Self::connection_handle_udp_packet(self.logger.clone(), con, addr, udp_packet, header, pos)
                    .map_err(|e| {
                        error!(logger, "Error handling udp packed"; "error" => ?e);
                    }));
                Box::new(future::ok(()))
            }
        } else {
            // Unknown connection
            if let Some(sink) = self.unknown_udp_packet_sink {
                // Don't block if the queue is full
                if sink.try_send((addr, udp_packet)).is_err() {
                    warn!(self.logger, "Unknown connection handler overloaded \
                        – dropping udp packet");
                }
            } else {
                warn!(self.logger, "Dropped packet without connection because \
                    no unknown packet handler is set");
            }
            Box::new(future::ok(()))
        }
    }

    /// Handle a packet for a specific connection.
    ///
    /// This part does the defragmentation, decryption and decompression.
    pub fn connection_handle_udp_packet(
        logger: Logger,
        connection: futures_locks::Mutex<(CM::AssociatedData, Connection)>,
        addr: SocketAddr,
        udp_packet: UdpPacket,
        header: packets::Header,
        pos: usize,
    ) -> impl Future<Item=(), Error=Error> {
        connection.with(move |con| {
            let mut udp_packet = &udp_packet.0[pos..];
            let packets = if let Some(ref mut params) = con.1.params {
                if header.get_type() == PacketType::Init {
                    return Err(Error::UnexpectedInitPacket);
                }
                let p_type = header.get_type();
                let type_i = p_type.to_usize().unwrap();
                let id = header.p_id;

                let mut ack = None;
                let (in_recv_win, gen_id, cur_next, limit) =
                    params.in_receive_window(p_type, id);
                // Ignore range for acks
                let res = if p_type == PacketType::Ack
                    || p_type == PacketType::AckLow
                    || in_recv_win
                {
                    let mut dec_data;
                    if !header.get_unencrypted() {
                        // If it is the first ack packet of a
                        // client, try to fake decrypt it.
                        let decrypted = if header.get_type() == PacketType::Ack
                            && header.p_id == 0
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
                            dec_data = algs::decrypt(
                                &header,
                                udp_packet,
                                gen_id,
                                &params.shared_iv,
                                &mut params.key_cache,
                            )?;
                            udp_packet = &dec_data;
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
                                    packets::Data::Ack(header.p_id),
                                ));
                            } else if header.get_type()
                                == PacketType::CommandLow
                            {
                                ack = Some((
                                    PacketType::AckLow,
                                    packets::Data::AckLow(
                                        header.p_id,
                                    ),
                                ));
                            }
                            Self::handle_command_packet(
                                &logger,
                                params,
                                header,
                                udp_packet,
                            )?
                        }
                        _ => {
                            if header.get_type() == PacketType::Ping {
                                ack = Some((
                                    PacketType::Pong,
                                    packets::Data::Pong(
                                        header.p_id,
                                    ),
                                ));
                            }
                            // Update packet ids
                            let id = id.wrapping_add(1);
                            params.incoming_p_ids[type_i] = (gen_id, id);

                            let p_data = packets::Data::read(
                                &header,
                                &mut Cursor::new(&udp_packet),
                            )?;
                            match p_data {
                                packets::Data::Ack(p_id) |
                                packets::Data::AckLow(p_id) => {
                                    // Remove command packet from send queue if the fitting ack is received.
                                    let p_type = if header.get_type()
                                        == PacketType::Ack {
                                        PacketType::Command
                                    } else {
                                        PacketType::CommandLow
                                    };
                                    con.1.resender.ack_packet(p_type, p_id);
                                    vec![Packet::new(header, p_data)]
                                }
                                packets::Data::VoiceS2C { .. } |
                                packets::Data::VoiceWhisperS2C { .. } => {
                                    // Seems to work better without assembling the first 3 voice packets
                                    // Use handle_voice_packet to assemble fragmented voice packets
                                    /*let mut res = Self::handle_voice_packet(&logger, params, &header, p_data);
                                    let res = res.drain(..).map(|p|
                                        (con_key.clone(), p)).collect();
                                    Ok(res)*/
                                    vec![Packet::new(header, p_data)]
                                }
                                _ => vec![Packet::new(header, p_data)],
                            }
                        }
                    }
                } else {
                    // Send an ack for the case when it was lost
                    if header.get_type() == PacketType::Command {
                        ack = Some((
                            PacketType::Ack,
                            packets::Data::Ack(header.p_id),
                        ));
                    } else if header.get_type() == PacketType::CommandLow {
                        ack = Some((
                            PacketType::AckLow,
                            packets::Data::AckLow(header.p_id),
                        ));
                    }
                    return Err(Error::NotInReceiveWindow {
                        id,
                        next: cur_next,
                        limit,
                        p_type: header.get_type(),
                    });
                };
                if let Some((ack_type, ack_packet)) = ack {
                    let mut ack_header = Header::default();
                    ack_header.set_type(ack_type);
                    tokio::spawn(con.1.send_packet(Packet::new(ack_header, ack_packet))
                        .map_err(move |e| {
                            error!(logger, "Failed to send ack packet"; "error" => ?e);
                        }));
                }
                res
            } else {
                // Try to fake decrypt the packet
                let udp_packet_bak = udp_packet.clone();
                if algs::decrypt_fake(&header, &mut udp_packet).is_ok() {
                    // Send ack
                    if header.get_type() == PacketType::Command {
                        let mut ack_header = Header::default();
                        ack_header.set_type(PacketType::Ack);
                        tokio::spawn(con.1.send_packet(Packet::new(ack_header,
                            packets::Data::Ack(header.p_id)))
                            .map_err(move |e| {
                                error!(logger, "Failed to send ack packet"; "error" => ?e);
                            }));
                    }
                } else {
                    udp_packet.copy_from_slice(&udp_packet_bak);
                }
                let p_data = packets::Data::read(
                    &header,
                    &mut Cursor::new(&*udp_packet),
                )?;
                vec![Packet::new(header, p_data)]
            };

            // TODO Do something with packets
            Ok(())
        }).unwrap()
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
                        frag_queue.extend_from_slice(udp_packet);
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
                        let p_data = packets::Data::read(
                            &header,
                            &mut Cursor::new(decompressed.as_slice()),
                        )?;
                        Some(Packet::new(header, p_data))
                    } else {
                        // Enqueue
                        *frag_queue = Some((header, udp_packet.to_vec()));
                        None
                    }
                } else if let Some((_, ref mut frag_queue)) = *frag_queue {
                    // The packet is fragmented
                    if frag_queue.len() < MAX_FRAGMENTS_LENGTH {
                        frag_queue.extend_from_slice(udp_packet);
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
                        udp_packet.to_vec()
                    };
                    /*if header.get_compressed() {
                        debug!(logger, "Decompressed"; "data" => ?::HexSlice(&decompressed));
                    }*/
                    let p_data = packets::Data::read(
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
                    udp_packet = &p;
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
        packet: packets::Data,
    ) -> Vec<Packet> {
        let cmd_i = if header.get_type() == PacketType::Voice {
            0
        } else {
            1
        };
        let frag_queue = &mut params.voice_fragmented_queue[cmd_i];

        let (id, from_id, codec_type, voice_data) = match packet {
            packets::Data::VoiceS2C { id, from_id, codec_type, voice_data } => (id, from_id, codec_type, voice_data),
            packets::Data::VoiceWhisperS2C { id, from_id, codec_type, voice_data } => (id, from_id, codec_type, voice_data),
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
                packets::Data::VoiceS2C { id, from_id, codec_type, voice_data: frags }
            } else {
                packets::Data::VoiceWhisperS2C { id, from_id, codec_type, voice_data: frags }
            };
            res.push(Packet::new(header.clone(), packet_data));
        }

        let packet_data = if header.get_type() == PacketType::Voice {
            packets::Data::VoiceS2C { id, from_id, codec_type, voice_data }
        } else {
            packets::Data::VoiceWhisperS2C { id, from_id, codec_type, voice_data }
        };
        res.push(Packet::new(header.clone(), packet_data));
        res
    }*/
}


pub struct PacketCodecSink<
    CM: ConnectionManager + 'static,
    Inner: Sink<SinkItem = (SocketAddr, UdpPacket), SinkError = Error>,
> {
    data: Weak<RefCell<Data<CM>>>,
    is_client: bool,
    inner: Inner,
    /// The type of the packets in the current send buffer.
    command_p_type: PacketType,
    connection_key: Option<CM::Key>,
    addr: Option<SocketAddr>,
    /// Currently buffered id and packet.
    command_send_buffer: Vec<(u16, UdpPacket)>,
    /// Send buffer for other packets
    other_send_buffer: Vec<(u16, UdpPacket)>,
}

impl<CM: ConnectionManager + 'static> PacketCodecSink<CM,
    ::handler_data::DataUdpPackets<CM>> {
    fn new(data: &Rc<RefCell<Data<CM>>>) -> Self {
        let is_client = data.borrow().is_client;
        Self {
            data: Rc::downgrade(data),
            is_client,
            inner: Data::get_udp_packets(Rc::downgrade(data)),
            command_p_type: PacketType::Command,
            connection_key: None,
            addr: None,
            command_send_buffer: Vec::new(),
            other_send_buffer: Vec::new(),
        }
    }

    /// Add a packet codec sink to the connection.
    pub fn apply(data: &Rc<RefCell<Data<CM>>>) {
        let sink = Self::new(data);
        data.borrow_mut().packet_sink = Some(Box::new(sink));
    }
}

impl<
    CM: ConnectionManager,
    Inner: Sink<SinkItem = (SocketAddr, UdpPacket), SinkError = Error>,
> Sink for PacketCodecSink<CM, Inner> {
    type SinkItem = (CM::Key, Packet);
    type SinkError = Error;

    fn start_send(
        &mut self,
        (con_key, mut packet): Self::SinkItem,
    ) -> futures::StartSend<Self::SinkItem, Self::SinkError> {
        // Check if there are unsent packets in the queue for this packet type
        let p_type = packet.header.get_type();
        if p_type == PacketType::Command || p_type == PacketType::CommandLow {
            if !self.command_send_buffer.is_empty() {
                self.poll_complete()?;
                if !self.command_send_buffer.is_empty() {
                    return Ok(futures::AsyncSink::NotReady((con_key, packet)));
                }
            }
        } else if !self.other_send_buffer.is_empty() {
            self.poll_complete()?;
            if !self.other_send_buffer.is_empty() {
                return Ok(futures::AsyncSink::NotReady((con_key, packet)));
            }
        }

        // The resulting list of udp packets
        let addr;
        let mut packets = {
            let is_client = self.is_client;
            let use_newprotocol =
                (packet.header.get_type() == PacketType::Command
                || packet.header.get_type() == PacketType::CommandLow)
                && is_client;

            // Get connection
            let con = {
                let data = self.data.upgrade().unwrap();
                let data = data.borrow();
                if let Some(con) = data.connection_manager.get_connection(con_key.clone()) {
                    con
                } else {
                    error!(data.logger, "Sending packet to non-existing connection");
                    return Ok(futures::AsyncSink::Ready);
                }
            };

            // Get the connection parameters
            let mut con = con.borrow_mut();
            addr = con.address;
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
                    && ((!is_client && p_id == 0)
                        || (is_client && p_id == 1 && {
                            // Test if it is a clientek packet
                            if let ::packets::Data::Command(ref cmd) =
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
                    algs::compress_and_split(is_client, &packet)
                } else {
                    // Set the inner packet id for voice packets
                    match packet.data {
                        ::packets::Data::VoiceC2S { ref mut id, .. }           |
                        ::packets::Data::VoiceS2C { ref mut id, .. }           |
                        ::packets::Data::VoiceWhisperC2S { ref mut id, .. }    |
                        ::packets::Data::VoiceWhisperNewC2S { ref mut id, .. } |
                        ::packets::Data::VoiceWhisperS2C { ref mut id, .. } => {
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
                        if is_client {
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
                                algs::encrypt_fake(&mut header, &mut p_data)?;
                            } else {
                                algs::encrypt(
                                    &mut header,
                                    &mut p_data,
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
                        Ok((header.p_id, UdpPacket(buf.into())))
                    })
                    .collect::<Result<Vec<_>>>()?;
                packets
            } else {
                // No connection params available
                let mut p_data = Vec::new();
                packet.data.write(&mut p_data).unwrap();
                let mut header = packet.header;
                // Client id for clients
                if is_client {
                    header.c_id = Some(0);
                } else {
                    header.c_id = None;
                }
                // Fake encrypt if needed
                if algs::should_encrypt(header.get_type(), false) {
                    header.set_unencrypted(false);
                    algs::encrypt_fake(&mut header, &mut p_data)?;
                }

                let mut buf = Vec::new();
                header.write(&mut buf)?;
                buf.append(&mut p_data);
                vec![(header.p_id, UdpPacket(buf.into()))]
            }
        };
        // Add the packets to the queue
        packets.reverse();
        // TODO Send init packets using the resender
        // TODO The following init packet has to be handled as an ack packet.
        if p_type == PacketType::Command || p_type == PacketType::CommandLow {
            self.connection_key = Some(con_key);
            self.command_send_buffer = packets;
            self.command_p_type = p_type;
        } else {
            self.addr = Some(addr);
            self.other_send_buffer = packets;
        }
        Ok(futures::AsyncSink::Ready)
    }

    fn poll_complete(&mut self) -> futures::Poll<(), Self::SinkError> {
        // Check if there are unsent command packets in the queue
        let command_res = {
            if let Some((p_id, packet)) = self.command_send_buffer.pop() {
                // Get the connection
                let data = if let Some(data) = self.data.upgrade() {
                    data
                } else {
                    return Err(format_err!("Data does not exist").into());
                };
                let data = data.borrow();
                if let Some(con) = data.connection_manager
                    .get_connection(self.connection_key.as_ref()
                        .expect("Forgot to set the connection key").clone()) {
                    let mut con = con.borrow_mut();
                    if let futures::AsyncSink::NotReady((_, p_id, packet)) =
                        con.resender.start_send((self.command_p_type, p_id, packet))?
                    {
                        self.command_send_buffer.push((p_id, packet));
                    } else {
                        futures::task::current().notify();
                    }
                    futures::Async::NotReady
                } else {
                    warn!(data.logger, "Cannot send packet for missing connection");
                    futures::Async::Ready(())
                }
            } else {
                futures::Async::Ready(())
            }
        };

        // Check if there are other unsent packets in the queue
        let other_res = {
            if let Some((p_id, packet)) = self.other_send_buffer.pop() {
                if let futures::AsyncSink::NotReady((_, packet)) =
                    self.inner.start_send((self.addr
                        .expect("Forgot to set the address"), packet))?
                {
                    self.other_send_buffer.push((p_id, packet));
                } else {
                    futures::task::current().notify();
                }
                futures::Async::NotReady
            } else {
                self.inner.poll_complete()?
            }
        };

        if command_res.is_not_ready() || other_res.is_not_ready() {
            Ok(futures::Async::NotReady)
        } else {
            Ok(futures::Async::Ready(()))
        }
    }

    fn close(&mut self) -> futures::Poll<(), Self::SinkError> {
        self.inner.close()
    }
}

pub struct AckHandler<
    T,
    UsedSink: Sink<SinkItem = (T, Packet), SinkError = Error> + 'static,
    InnerStream: Stream<Item = (Option<(T, Packet)>, Option<(T, Packet)>),
        Error = Error> + 'static,
> {
    used_sink: UsedSink,
    inner_stream: InnerStream,
    /// A buffer for an ack packet.
    ack_buffer: Option<(T, Packet)>,
    /// If we have put a packet into the sink and should poll for completion.
    should_poll_complete: bool,
}

impl<
    T,
    UsedSink: Sink<SinkItem = (T, Packet), SinkError = Error> + 'static,
    InnerStream: Stream<Item = (Option<(T, Packet)>, Option<(T, Packet)>),
        Error = Error> + 'static,
> AckHandler<T, UsedSink, InnerStream> {
    pub fn new(inner_stream: InnerStream, used_sink: UsedSink) -> Self {
        Self {
            used_sink,
            inner_stream,
            ack_buffer: None,
            should_poll_complete: false,
        }
    }
}

impl<
    T,
    UsedSink: Sink<SinkItem = (T, Packet), SinkError = Error> + 'static,
    InnerStream: Stream<Item = (Option<(T, Packet)>, Option<(T, Packet)>),
        Error = Error> + 'static,
> Stream for AckHandler<T, UsedSink, InnerStream> {
    type Item = (T, Packet);
    type Error = Error;

    fn poll(&mut self) -> futures::Poll<Option<Self::Item>, Self::Error> {
        // Try to send the ack buffer
        if let Some(p) = self.ack_buffer.take() {
            if let futures::AsyncSink::NotReady(p) =
                self.used_sink.start_send(p)? {
                self.ack_buffer = Some(p);
                return Ok(futures::Async::NotReady);
            } else {
                self.should_poll_complete = true;
            }
        }

        if self.should_poll_complete {
            if let futures::Async::Ready(()) = self.used_sink.poll_complete()? {
                self.should_poll_complete = false;
            }
        }

        match self.inner_stream.poll()? {
            futures::Async::Ready(Some((packet, ack))) => {
                if let Some(p) = ack {
                    // Try to send the ack
                    if let futures::AsyncSink::NotReady(p) =
                        self.used_sink.start_send(p)? {
                        self.ack_buffer = Some(p);
                    } else if let futures::Async::NotReady =
                        self.used_sink.poll_complete()? {
                        self.should_poll_complete = true;
                    }
                }

                if let Some(packet) = packet {
                    Ok(futures::Async::Ready(Some(packet)))
                } else {
                    task::current().notify();
                    Ok(futures::Async::NotReady)
                }
            }
            futures::Async::Ready(None) => Ok(futures::Async::Ready(None)),
            futures::Async::NotReady => Ok(futures::Async::NotReady),
        }
    }
}
