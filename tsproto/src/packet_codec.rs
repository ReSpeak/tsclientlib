use std::cell::RefCell;
use std::io::Cursor;
use std::mem;
use std::net::{self, SocketAddr};
use std::rc::{Rc, Weak};
use std::u16;

use chrono::Utc;
use futures::{self, future, Future, Sink, Stream};
use futures::task;
use num::ToPrimitive;
use slog;

use {
    packets, BoxFuture, Error, Result, ResultExt, MAX_FRAGMENTS_LENGTH,
    MAX_QUEUE_LEN,
};
use algorithms as algs;
use handler_data::{ConnectedParams, Data};
use packets::*;

pub struct PacketCodecStream<
    CS,
    Inner: Stream<Item = (SocketAddr, UdpPacket), Error = Error>,
> {
    data: Weak<RefCell<Data<CS>>>,
    inner: Inner,
    receive_buffer: Vec<Packet>,
    receive_addr: SocketAddr,
    ack_packet: Option<(SocketAddr, Packet)>,
}

impl<CS, Inner: Stream<Item = (SocketAddr, UdpPacket), Error = Error>>
    PacketCodecStream<CS, Inner> {
    pub fn new(data: Rc<RefCell<Data<CS>>>, inner: Inner) -> Self {
        Self {
            data: Rc::downgrade(&data),
            inner,
            receive_buffer: Vec::new(),
            receive_addr: SocketAddr::new(
                net::IpAddr::V4(net::Ipv4Addr::new(0, 0, 0, 0)),
                0,
            ),
            ack_packet: None,
        }
    }

    /// Handle `Command` and `CommandLow` packets.
    ///
    /// They have to be handled in the right order.
    fn handle_command_packet(
        logger: slog::Logger,
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
                            debug!(logger, "Decompressed"; "data" => ?::HexSlice(&decompressed));
                        }*/
                        let p_data = packets::Data::read(
                            &header,
                            &mut Cursor::new(decompressed),
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
                        bail!("Too many fragments");
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
                        &mut Cursor::new(decompressed),
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
            warn!(logger, "Out of order command packet"; "got" => id, "expected" => cur_next);
            let limit = ((u32::from(cur_next) + MAX_QUEUE_LEN as u32)
                % u32::from(u16::MAX)) as u16;
            if (cur_next < limit && id >= cur_next && id < limit)
                || (cur_next > limit && (id >= cur_next || id < limit))
            {
                r_queue.push((header, packet.0));
                Ok(vec![])
            } else {
                Err("Max queue length for commands exceeded".into())
            }
        }
    }
}

impl<CS: 'static> PacketCodecStream<CS, ::handler_data::DataUdpPackets<CS>> {
    /// Add a packet codec stream to the data.
    ///
    /// `Ack`, `AckLow` and `Pong` packets can be suppressed by setting
    /// `send_acks` to `false`.
    pub fn apply(data: Rc<RefCell<Data<CS>>>, send_acks: bool) {
        let stream =
            Self::new(data.clone(), Data::get_udp_packets(data.clone()));
        {
            let data2 = data.clone();
            let mut data = data.borrow_mut();
            if send_acks {
                let sink = data.packet_sink.take().unwrap();
                let stream = AckHandler::new(stream, Data::get_packets(data2));
                data.packet_stream = Some(Box::new(stream));
                data.packet_sink = Some(Box::new(sink));
            } else {
                let stream = Box::new(stream.filter_map(|(p, _)| p));
                data.packet_stream = Some(stream);
            }
        }
    }
}

impl<
    CS: 'static,
    Inner: Stream<Item = (SocketAddr, UdpPacket), Error = Error>,
> Stream for PacketCodecStream<CS, Inner> {
    /// Source address, source packet, optional ack packet if one should be
    /// sent.
    type Item = (Option<(SocketAddr, Packet)>, Option<(SocketAddr, Packet)>);
    type Error = Error;

    fn poll(&mut self) -> futures::Poll<Option<Self::Item>, Self::Error> {
        // Check if there are packets in the queue
        if !self.receive_buffer.is_empty() || self.ack_packet.is_some() {
            return Ok(futures::Async::Ready(Some((
                self.receive_buffer.pop().map(|p| (self.receive_addr, p)),
                self.ack_packet.take(),
            ))));
        }
        // Don't return an error, that will terminate the stream (log them only)
        let res: Result<_> = match self.inner.poll()? {
            // Wrap in a closure so try! can be used
            futures::Async::Ready(Some((addr, UdpPacket(mut udp_packet)))) => {
                (|| {
                    let data = if let Some(data) = self.data.upgrade() {
                        data
                    } else {
                        return Ok(futures::Async::Ready(None));
                    };
                    let mut data = data.borrow_mut();
                    let is_client = data.is_client;
                    let (header, pos) = {
                        let mut r = Cursor::new(&udp_packet);
                        (
                            packets::Header::read(&!data.is_client, &mut r)?,
                            r.position() as usize,
                        )
                    };
                    let mut udp_packet = udp_packet.split_off(pos);

                    let packets: Vec<Packet> = {
                        let logger = data.logger.clone();
                        let data = &mut *data;
                        let mut update_rtt = None;
                        let res = if let Some(params) = data.connections
                            .get_mut(&addr)
                            .and_then(|con| con.params.as_mut())
                        {
                            if header.get_type() == PacketType::Init {
                                bail!("Unexpected init packet")
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
                                    let decrypted = if header.get_type()
                                        == PacketType::Ack
                                        && header.p_id == 0
                                        && !is_client
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
                                        ).chain_err(|| "Packet decryption failed")?
                                    }
                                } else if algs::must_encrypt(header.get_type())
                                {
                                    // Check if it is ok for the packet to be
                                    // unencrypted
                                    bail!("Got unallowed unencrypted packet");
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
                                            logger,
                                            params,
                                            header,
                                            UdpPacket(udp_packet),
                                        );
                                        // Don't send an ack if an error is
                                        // returned
                                        if res.is_err() {
                                            ack = None;
                                        }
                                        res
                                    }
                                    _ => {
                                        if header.get_type() == PacketType::Ping
                                        {
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
                                            &mut Cursor::new(udp_packet),
                                        ) {
                                            Ok(p_data) => {
                                                // Remove command packet from send queue if the fitting ack is received.
                                                match p_data {
                                                    packets::Data::Ack(
                                                        p_id,
                                                    ) |
                                                    packets::Data::AckLow(
                                                        p_id,
                                                    ) => {
                                                        // Remove from send queue
                                                        let p_type = if header
                                                            .get_type()
                                                            == PacketType::Ack
                                                        {
                                                            PacketType::Command
                                                        } else {
                                                            PacketType::CommandLow
                                                        };
                                                        let mut rec = None;
                                                        let mut items = data.send_queue.drain()
                                                            .filter_map(|r| if r.p_type == p_type && r.p_id == p_id {
                                                                rec = Some(r);
                                                                None
                                                            } else {
                                                                Some(r)
                                                            })
                                                            .collect();
                                                        mem::swap(&mut items, &mut data.send_queue);
                                                        // Update smoothed round trip time
                                                        if let Some(rec) = rec {
                                                            // Only if it was not resent
                                                            if rec.tries == 1 {
                                                                let now =
                                                                    Utc::now();
                                                                let diff = now.naive_utc().signed_duration_since(rec.sent.naive_utc());
                                                                update_rtt =
                                                                    Some(diff);
                                                            }
                                                        }
                                                    }
                                                    _ => {}
                                                }
                                                Ok(vec![
                                                    Packet::new(header, p_data),
                                                ])
                                            }
                                            Err(error) => Err(error.into()),
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
                                } else if header.get_type()
                                    == PacketType::CommandLow
                                {
                                    ack = Some((
                                        PacketType::AckLow,
                                        packets::Data::AckLow(header.p_id),
                                    ));
                                }
                                Err(
                                    format!(
                                        "Packet {} not in receive window \
                                         [{};{}) for type {:?}",
                                        id,
                                        cur_next,
                                        limit,
                                        header.get_type(),
                                    ).into(),
                                )
                            };
                            if let Some((ack_type, ack_packet)) = ack {
                                let mut ack_header = Header::default();
                                ack_header.set_type(ack_type);
                                self.ack_packet = Some(
                                    (addr, Packet::new(ack_header, ack_packet)),
                                );
                            }
                            res
                        } else {
                            // Try to fake decrypt the initivexpand packet
                            if header.get_type() == PacketType::Command
                                && is_client
                            {
                                let udp_packet_bak = udp_packet.clone();
                                if algs::decrypt_fake(&header, &mut udp_packet)
                                    .is_ok()
                                {
                                    // Send ack
                                    let mut ack_header = Header::default();
                                    ack_header.set_type(PacketType::Ack);
                                    self.ack_packet = Some((
                                        addr,
                                        Packet::new(
                                            ack_header,
                                            packets::Data::Ack(header.p_id),
                                        ),
                                    ));
                                } else {
                                    udp_packet.copy_from_slice(&udp_packet_bak);
                                }
                            }
                            let p_data = packets::Data::read(
                                &header,
                                &mut Cursor::new(udp_packet),
                            )?;
                            Ok(vec![Packet::new(header, p_data)])
                        };
                        if let Some(rtt) = update_rtt {
                            if let Some(con) = data.connections.get_mut(&addr) {
                                con.update_srtt(rtt);
                            }
                        }
                        res
                    }?;

                    self.receive_addr = addr;
                    self.receive_buffer = packets;
                    if !self.receive_buffer.is_empty()
                        || self.ack_packet.is_some()
                    {
                        if self.receive_buffer.len() > 1 {
                            futures::task::current().notify();
                        }
                        Ok(futures::Async::Ready(Some((
                            self.receive_buffer
                                .pop()
                                .map(|p| (self.receive_addr, p)),
                            self.ack_packet.take(),
                        ))))
                    } else {
                        // Wait for more packets
                        task::current().notify();
                        Ok(futures::Async::NotReady)
                    }
                })()
            }
            futures::Async::Ready(None) => Ok(futures::Async::Ready(None)),
            futures::Async::NotReady => Ok(futures::Async::NotReady),
        };

        if let Err(error) = res {
            // Log error
            let description = error.description();
            let data = if let Some(data) = self.data.upgrade() {
                data
            } else {
                return Ok(futures::Async::Ready(None));
            };
            error!(data.borrow().logger,
                "Receiving packet"; "error" => description);
            // Wait for more packets
            task::current().notify();
            Ok(futures::Async::NotReady)
        } else {
            res
        }
    }
}


pub struct PacketCodecSink<
    CS,
    Inner: Sink<SinkItem = (SocketAddr, UdpPacket), SinkError = Error>,
> {
    data: Weak<RefCell<Data<CS>>>,
    inner: Inner,
    send_buffer: Vec<UdpPacket>,
    send_addr: SocketAddr,
}

impl<
    CS: 'static,
    Inner: Sink<SinkItem = (SocketAddr, UdpPacket), SinkError = Error>,
> PacketCodecSink<CS, Inner> {
    pub fn new(data: Rc<RefCell<Data<CS>>>, inner: Inner) -> Self {
        Self {
            data: Rc::downgrade(&data),
            inner,
            send_buffer: Vec::new(),
            send_addr: SocketAddr::new(
                net::IpAddr::V4(net::Ipv4Addr::new(0, 0, 0, 0)),
                0,
            ),
        }
    }
}

impl<CS: 'static> PacketCodecSink<CS, ::handler_data::DataUdpPackets<CS>> {
    /// Add a packet codec sink to the data.
    pub fn apply(data: Rc<RefCell<Data<CS>>>) {
        let sink = Self::new(data.clone(), Data::get_udp_packets(data.clone()));
        data.borrow_mut().packet_sink = Some(Box::new(sink));
    }
}

impl<
    CS: 'static,
    Inner: Sink<SinkItem = (SocketAddr, UdpPacket), SinkError = Error>,
> Sink for PacketCodecSink<CS, Inner> {
    type SinkItem = (SocketAddr, Packet);
    type SinkError = Error;

    fn start_send(
        &mut self,
        (addr, packet): Self::SinkItem,
    ) -> futures::StartSend<Self::SinkItem, Self::SinkError> {
        // Check if there are unsent packets in the queue
        if self.poll_complete()? == futures::Async::NotReady {
            return Ok(futures::AsyncSink::NotReady((addr, packet)));
        }

        // Check if the send queue is overloaded
        let send_queue_overloaded = {
            let data = self.data.upgrade().unwrap();
            let data = data.borrow();
            data.send_queue.len() >= ::MAX_SEND_QUEUE_LEN
        };
        if send_queue_overloaded {
            return Ok(futures::AsyncSink::NotReady((addr, packet)));
        }

        let p_type = packet.header.get_type();
        let mut start_p_id = 0;
        let mut packets = {
            let data = self.data.upgrade().unwrap();
            let mut data = data.borrow_mut();
            let is_client = data.is_client;
            let use_newprotocol = (packet.header.get_type()
                == PacketType::Command
                || packet.header.get_type() == PacketType::CommandLow)
                && data.is_client;

            if let Some(con) = data.connections.get_mut(&addr) {
                if let Some(params) = con.params.as_mut() {
                    let p_type = packet.header.get_type();
                    let type_i = p_type.to_usize().unwrap();
                    start_p_id = params.outgoing_p_ids[type_i].1;
                    // Compress and split packet
                    let mut packets = if p_type == PacketType::Command
                        || p_type == PacketType::CommandLow
                    {
                        algs::compress_and_split(&packet)
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
                                if header.get_type() == PacketType::Command
                                    && !is_client && header.p_id == 0
                                {
                                    algs::encrypt_fake(&mut header, &mut p_data)?;
                                } else {
                                    algs::encrypt(
                                        &mut header,
                                        &mut p_data,
                                        gen,
                                        &params.shared_iv,
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
                            Ok(UdpPacket(buf))
                        })
                        .collect::<Result<Vec<_>>>()?;
                    packets
                } else {
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
                    vec![UdpPacket(buf)]
                }
            } else {
                // We are not yet connected, so do nothing
                let mut buf = Vec::new();
                packet.write(&mut buf)?;
                vec![UdpPacket(buf)]
            }
        };
        // Add command packets to send queue instead of sending them
        if p_type.is_command() {
            for p in packets {
                Data::add_outgoing_packet(
                    self.data.upgrade().unwrap(),
                    p_type,
                    start_p_id,
                    (addr, p),
                );
                start_p_id = start_p_id.wrapping_add(1);
            }
        } else {
            packets.reverse();
            self.send_buffer = packets;
            self.send_addr = addr;
        }
        Ok(futures::AsyncSink::Ready)
    }

    fn poll_complete(&mut self) -> futures::Poll<(), Self::SinkError> {
        // Check if there are unsent packets in the queue
        if let Some(packet) = self.send_buffer.pop() {
            if let futures::AsyncSink::NotReady((_, packet)) =
                self.inner.start_send((self.send_addr, packet))?
            {
                self.send_buffer.push(packet);
            } else {
                futures::task::current().notify();
            }
            Ok(futures::Async::NotReady)
        } else {
            self.inner.poll_complete()
        }
    }

    fn close(&mut self) -> futures::Poll<(), Self::SinkError> {
        self.inner.close()
    }
}

pub struct AckHandler {
    inner_stream: Box<Stream<Item = (SocketAddr, Packet), Error = Error>>,
}

impl AckHandler {
    pub fn new<
        UsedSink: Sink<SinkItem = (SocketAddr, Packet), SinkError = Error> + 'static,
        InnerStream: Stream<
            Item = (Option<(SocketAddr, Packet)>, Option<(SocketAddr, Packet)>),
            Error = Error,
        >
            + 'static,
    >(
        inner_stream: InnerStream,
        sink: UsedSink,
    ) -> Self {
        let sink = Rc::new(RefCell::new(Some(sink)));
        let inner_stream =
            Box::new(
                inner_stream
                    .and_then(
                        move |(packet, ack)| -> BoxFuture<
                            Option<(SocketAddr, Packet)>,
                            Error,
                        > {
                            // Check if we should send an ack packet
                            if let Some(ack) = ack {
                                // Take sink
                                let tmp_sink =
                                    sink.borrow_mut().take().unwrap();
                                // Send the ack
                                let sink = sink.clone();
                                Box::new(
                                    tmp_sink.send(ack).map(move |tmp_sink| {
                                        *sink.borrow_mut() = Some(tmp_sink);
                                        // Return the packet
                                        packet
                                    }),
                                )
                            } else {
                                Box::new(future::ok(packet))
                            }
                        },
                    )
                    .filter_map(|o| o),
            );
        Self { inner_stream }
    }
}

impl Stream for AckHandler {
    type Item = (SocketAddr, Packet);
    type Error = Error;

    fn poll(&mut self) -> futures::Poll<Option<Self::Item>, Self::Error> {
        self.inner_stream.poll()
    }
}
