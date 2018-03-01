use std::cell::RefCell;
use std::io::Cursor;
use std::rc::{Rc, Weak};
use std::u16;

use futures::{self, Sink, Stream};
use futures::task;
use num::ToPrimitive;
use slog;

use {packets, Error, Result, MAX_FRAGMENTS_LENGTH, MAX_QUEUE_LEN };
use algorithms as algs;
use connection::{Connection, ConnectedParams};
use connectionmanager::{ConnectionManager, Resender};
use packets::*;

pub struct PacketCodecStream<
    CM: ConnectionManager + 'static,
    Inner: Stream<Item = UdpPacket, Error = Error>,
> {
    connection: Weak<RefCell<Connection<CM>>>,
    is_client: bool,
    inner: Inner,
    receive_buffer: Vec<Packet>,
    ack_packet: Option<Packet>,
}

impl<CM: ConnectionManager, Inner: Stream<Item = UdpPacket, Error = Error>>
    PacketCodecStream<CM, Inner> {
    pub fn new(connection: &Rc<RefCell<Connection<CM>>>, inner: Inner) -> Self {
        let is_client = connection.borrow().is_client;
        Self {
            connection: Rc::downgrade(connection),
            is_client,
            inner,
            receive_buffer: Vec::new(),
            ack_packet: None,
        }
    }

    /// Handle `Command` and `CommandLow` packets.
    ///
    /// They have to be handled in the right order.
    fn handle_command_packet(
        logger: &slog::Logger,
        params: &mut ConnectedParams,
        mut header: Header,
        mut packet: UdpPacket,
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
                        frag_queue.append(&mut packet.0);
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
                        *frag_queue = Some((header, packet.0));
                        None
                    }
                } else if let Some((_, ref mut frag_queue)) = *frag_queue {
                    // The packet is fragmented
                    if frag_queue.len() < MAX_FRAGMENTS_LENGTH {
                        frag_queue.append(&mut packet.0);
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
                            &mut Cursor::new(packet.0),
                            ::MAX_DECOMPRESSED_SIZE,
                        )?
                    } else {
                        packet.0
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
                    packet = UdpPacket(p);
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
                r_queue.push((header, packet.0));
                Ok(vec![])
            } else {
                Err(Error::MaxLengthExceeded(String::from("command queue")))
            }
        }
    }

    fn on_packet_received(&mut self, mut udp_packet: Vec<u8>)
        -> futures::Poll<Option<<Self as Stream>::Item>,
            <Self as Stream>::Error> {
        let con = if let Some(con) = self.connection.upgrade() {
            con
        } else {
            return Ok(futures::Async::Ready(None));
        };
        let con = &mut *con.borrow_mut();
        let is_client = self.is_client;
        let (header, pos) = {
            let mut r = Cursor::new(udp_packet.as_slice());
            (
                packets::Header::read(&!is_client, &mut r)?,
                r.position() as usize,
            )
        };
        let mut udp_packet = udp_packet.split_off(pos);

        let packets: Vec<Packet> = {
            let logger = con.logger.clone();
            if let Some(ref mut params) = con.params {
                if header.get_type() == PacketType::Init {
                    return Err(Error::UnexpectedInitPacket);
                }
                let p_type = header.get_type();
                let type_i = p_type.to_usize().unwrap();
                let id = header.p_id;

                let mut ack = None;
                let (in_recv_win, cur_next, limit) =
                    params.in_receive_window(p_type, id);
                // Ignore range for acks
                let res = if p_type == PacketType::Ack
                    || p_type == PacketType::AckLow
                    || in_recv_win
                {
                    let gen_id = params.incoming_p_ids[type_i].0;

                    if !header.get_unencrypted() {
                        // If it is the first ack packet of a
                        // client, try to fake decrypt it.
                        let decrypted = if header.get_type() == PacketType::Ack
                            && header.p_id == 0
                        {
                            let udp_packet_bak = udp_packet.clone();
                            if algs::decrypt_fake(
                                &header,
                                &mut udp_packet,
                            ).is_ok() {
                                true
                            } else {
                                udp_packet.copy_from_slice(&udp_packet_bak);
                                false
                            }
                        } else {
                            false
                        };
                        if !decrypted {
                            // Decrypt the packet
                            algs::decrypt(
                                &header,
                                &mut udp_packet,
                                gen_id,
                                &params.shared_iv,
                                &mut params.key_cache,
                            )?
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
                            let res = Self::handle_command_packet(
                                &logger,
                                params,
                                header,
                                UdpPacket(udp_packet),
                            );
                            // Don't send an ack if an error is returned
                            if res.is_err() {
                                ack = None;
                            }
                            res
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
                            if id < params.incoming_p_ids[type_i].1 {
                                params.incoming_p_ids[type_i].0 =
                                    gen_id.wrapping_add(1);
                            }
                            params.incoming_p_ids[type_i].1 = id;

                            match packets::Data::read(
                                &header,
                                &mut Cursor::new(udp_packet.as_slice()),
                            ) {
                                Ok(p_data) => {
                                    // Remove command packet from send queue if the fitting ack is received.
                                    match p_data {
                                        packets::Data::Ack(p_id) |
                                        packets::Data::AckLow(p_id) => {
                                            // Remove from send queue
                                            let p_type = if header.get_type()
                                                == PacketType::Ack {
                                                PacketType::Command
                                            } else {
                                                PacketType::CommandLow
                                            };
                                            con.resender.ack_packet(p_type, p_id);
                                        }
                                        _ => {}
                                    }
                                    Ok(vec![Packet::new(header, p_data)])
                                }
                                Err(error) => Err(error),
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
                    Err(Error::NotInReceiveWindow {
                        id,
                        next: cur_next,
                        limit,
                        p_type: header.get_type(),
                    })
                };
                if let Some((ack_type, ack_packet)) = ack {
                    let mut ack_header = Header::default();
                    ack_header.set_type(ack_type);
                    self.ack_packet = Some(Packet::new(ack_header, ack_packet));
                }
                res
            } else {
                // Try to fake decrypt the initivexpand packet
                if header.get_type() == PacketType::Command && is_client {
                    let udp_packet_bak = udp_packet.clone();
                    if algs::decrypt_fake(&header, &mut udp_packet).is_ok() {
                        // Send ack
                        let mut ack_header = Header::default();
                        ack_header.set_type(PacketType::Ack);
                        self.ack_packet = Some(Packet::new(
                                ack_header,
                                packets::Data::Ack(header.p_id),
                            )
                        );
                    } else {
                        udp_packet.copy_from_slice(&udp_packet_bak);
                    }
                }
                let p_data = packets::Data::read(
                    &header,
                    &mut Cursor::new(udp_packet.as_slice()),
                )?;
                Ok(vec![Packet::new(header, p_data)])
            }
        }?;

        self.receive_buffer = packets;
        if !self.receive_buffer.is_empty() || self.ack_packet.is_some() {
            if self.receive_buffer.len() > 1 {
                futures::task::current().notify();
            }
            Ok(futures::Async::Ready(Some((
                self.receive_buffer
                    .pop(),
                self.ack_packet.take(),
            ))))
        } else {
            // Wait for more packets
            task::current().notify();
            Ok(futures::Async::NotReady)
        }
    }
}

impl<CM: ConnectionManager + 'static> PacketCodecStream<CM,
    ::connection::UdpPackets<CM>> {
    /// Add a packet codec stream to the connection.
    ///
    /// `Ack`, `AckLow` and `Pong` packets can be suppressed by setting
    /// `send_acks` to `false`.
    pub fn apply(connection: &Rc<RefCell<Connection<CM>>>, send_acks: bool) {
        let stream = Self::new(connection,
            Connection::get_udp_packets(connection));
        let connection2 = connection.clone();
        let mut connection = connection.borrow_mut();
        let stream: Box<Stream<Item=_, Error=_>> = if send_acks {
            Box::new(AckHandler::new(stream,
                Connection::get_packets(&connection2)))
        } else {
            Box::new(stream.filter_map(|(p, _)| p))
        };

        connection.packet_stream = Some(stream);
    }
}

impl<
    CM: ConnectionManager,
    Inner: Stream<Item = UdpPacket, Error = Error>,
> Stream for PacketCodecStream<CM, Inner> {
    /// Source packet, optional ack packet if one should be sent.
    type Item = (Option<Packet>, Option<Packet>);
    type Error = Error;

    fn poll(&mut self) -> futures::Poll<Option<Self::Item>, Self::Error> {
        // Check if there are packets in the queue
        if !self.receive_buffer.is_empty() || self.ack_packet.is_some() {
            return Ok(futures::Async::Ready(Some((
                self.receive_buffer.pop(),
                self.ack_packet.take(),
            ))));
        }
        // Don't return an error, that will terminate the stream (log them only)
        let res: Result<_> = match self.inner.poll()? {
            futures::Async::Ready(Some(UdpPacket(mut udp_packet))) =>
                self.on_packet_received(udp_packet),
            futures::Async::Ready(None) => Ok(futures::Async::Ready(None)),
            futures::Async::NotReady => Ok(futures::Async::NotReady),
        };

        if let Err(error) = res {
            // Log error
            let logger = if let Some(con) = self.connection.upgrade() {
                con.borrow().logger.clone()
            } else {
                return Ok(futures::Async::Ready(None));
            };
            error!(logger, "Receiving packet"; "error" => %error);
            // Wait for more packets
            task::current().notify();
            Ok(futures::Async::NotReady)
        } else {
            res
        }
    }
}


pub struct PacketCodecSink<
    CM: ConnectionManager + 'static,
    Inner: Sink<SinkItem = UdpPacket, SinkError = Error>,
> {
    connection: Weak<RefCell<Connection<CM>>>,
    is_client: bool,
    inner: Inner,
    /// The type of the packets in the current send buffer.
    command_p_type: PacketType,
    /// Currently buffered id and packet.
    command_send_buffer: Vec<(u16, UdpPacket)>,
    /// Send buffer for other packets
    other_send_buffer: Vec<(u16, UdpPacket)>,
}

impl<CM: ConnectionManager + 'static> PacketCodecSink<CM,
    ::connection::UdpPackets<CM>> {
    fn new(connection: &Rc<RefCell<Connection<CM>>>) -> Self {
        let is_client = connection.borrow().is_client;
        Self {
            connection: Rc::downgrade(connection),
            is_client,
            inner: Connection::get_udp_packets(connection),
            command_p_type: PacketType::Command,
            command_send_buffer: Vec::new(),
            other_send_buffer: Vec::new(),
        }
    }

    /// Add a packet codec sink to the connection.
    pub fn apply(connection: &Rc<RefCell<Connection<CM>>>) {
        let sink = Self::new(connection);
        connection.borrow_mut().packet_sink = Some(Box::new(sink));
    }
}

impl<
    CM: ConnectionManager,
    Inner: Sink<SinkItem = UdpPacket, SinkError = Error>,
> Sink for PacketCodecSink<CM, Inner> {
    type SinkItem = Packet;
    type SinkError = Error;

    fn start_send(
        &mut self,
        packet: Self::SinkItem,
    ) -> futures::StartSend<Self::SinkItem, Self::SinkError> {
        // Check if there are unsent packets in the queue for this packet type
        let p_type = packet.header.get_type();
        if p_type == PacketType::Command || p_type == PacketType::CommandLow {
            if !self.command_send_buffer.is_empty() {
                self.poll_complete()?;
                if !self.command_send_buffer.is_empty() {
                    return Ok(futures::AsyncSink::NotReady(packet));
                }
            }
        } else if !self.other_send_buffer.is_empty() {
            self.poll_complete()?;
            if !self.other_send_buffer.is_empty() {
                return Ok(futures::AsyncSink::NotReady(packet));
            }
        }

        // The resulting list of udp packets
        let mut packets = {
            let is_client = self.is_client;
            let use_newprotocol =
                (packet.header.get_type() == PacketType::Command
                || packet.header.get_type() == PacketType::CommandLow)
                && is_client;

            // Get the connection parameters
            let con = self.connection.upgrade().unwrap();
            let mut con = con.borrow_mut();
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
                            if let Data::Command(ref cmd) =
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
                        Ok((header.p_id, UdpPacket(buf)))
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
                vec![(header.p_id, UdpPacket(buf))]
            }
        };
        // Add the packets to the queue
        packets.reverse();
        // TODO Send init packets using the resender
        // TODO The following init packet has to be handled as an ack packet.
        if p_type == PacketType::Command || p_type == PacketType::CommandLow {
            self.command_send_buffer = packets;
            self.command_p_type = p_type;
        } else {
            self.other_send_buffer = packets;
        }
        Ok(futures::AsyncSink::Ready)
    }

    fn poll_complete(&mut self) -> futures::Poll<(), Self::SinkError> {
        // Check if there are unsent command packets in the queue
        let command_res = {
            let con = self.connection.upgrade().unwrap();
            let mut con = con.borrow_mut();
            if let Some((p_id, packet)) = self.command_send_buffer.pop() {
                if let futures::AsyncSink::NotReady((_, p_id, packet)) =
                    con.resender.start_send((self.command_p_type, p_id, packet))?
                {
                    self.command_send_buffer.push((p_id, packet));
                } else {
                    futures::task::current().notify();
                }
                futures::Async::NotReady
            } else {
                con.resender.poll_complete()?
            }
        };

        // Check if there are other unsent packets in the queue
        let other_res = {
            if let Some((p_id, packet)) = self.other_send_buffer.pop() {
                if let futures::AsyncSink::NotReady(packet) =
                    self.inner.start_send(packet)?
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
    UsedSink: Sink<SinkItem = Packet, SinkError = Error> + 'static,
    InnerStream: Stream<Item = (Option<Packet>, Option<Packet>), Error = Error>
        + 'static,
> {
    used_sink: UsedSink,
    inner_stream: InnerStream,
    /// A buffer for an ack packet.
    ack_buffer: Option<Packet>,
    /// If we have put a packet into the sink and should poll for completion.
    should_poll_complete: bool,
}

impl<
    UsedSink: Sink<SinkItem = Packet, SinkError = Error> + 'static,
    InnerStream: Stream<Item = (Option<Packet>, Option<Packet>), Error = Error>
        + 'static,
> AckHandler<UsedSink, InnerStream> {
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
    UsedSink: Sink<SinkItem = Packet, SinkError = Error> + 'static,
    InnerStream: Stream<Item = (Option<Packet>, Option<Packet>), Error = Error>
        + 'static,
> Stream for AckHandler<UsedSink, InnerStream> {
    type Item = Packet;
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
