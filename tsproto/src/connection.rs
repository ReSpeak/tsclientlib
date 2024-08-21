use std::collections::VecDeque;
use std::io;
use std::mem;
use std::net::SocketAddr;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures::prelude::*;
use generic_array::typenum::consts::U16;
use generic_array::GenericArray;
use num_traits::ToPrimitive;
use tokio::io::ReadBuf;
use tokio::net::UdpSocket;
use tracing::{info_span, Span};
use tsproto_packets::packets::*;
use tsproto_types::crypto::EccKeyPubP256;

use crate::packet_codec::PacketCodec;
use crate::resend::{PacketId, PartialPacketId, Resender, ResenderState};
use crate::{Error, Result, MAX_UDP_PACKET_LENGTH, UDP_SINK_CAPACITY};

/// The needed functions, this can be used to abstract from the underlying
/// transport and allows simulation.
pub trait Socket {
	fn poll_recv_from(&self, cx: &mut Context, buf: &mut ReadBuf) -> Poll<io::Result<SocketAddr>>;
	fn poll_send_to(
		&self, cx: &mut Context, buf: &[u8], target: SocketAddr,
	) -> Poll<io::Result<usize>>;
	fn local_addr(&self) -> io::Result<SocketAddr>;
}

/// A cache for the key and nonce for a generation id.
/// This has to be stored for each packet type.
#[derive(Debug)]
pub struct CachedKey {
	pub generation_id: u32,
	pub key: GenericArray<u8, U16>,
	pub nonce: GenericArray<u8, U16>,
}

/// Data that has to be stored for a connection when it is connected.
pub struct ConnectedParams {
	/// The client id of this connection.
	pub c_id: u16,
	/// If voice packets should be encrypted
	pub voice_encryption: bool,

	/// The public key of the other side.
	pub public_key: EccKeyPubP256,
	/// The iv used to encrypt and decrypt packets.
	pub shared_iv: [u8; 64],
	/// The mac used for unencrypted packets.
	pub shared_mac: [u8; 8],
	/// Cached key and nonce per packet type and for server to client (without
	/// client id inside the packet) and client to server communication.
	pub key_cache: [[CachedKey; 2]; 8],
}

/// An event that originates from a tsproto raw connection.
#[derive(Debug)]
pub enum Event<'a> {
	ReceiveUdpPacket(&'a InUdpPacket<'a>),
	ReceivePacket(&'a InPacket<'a>),
	SendUdpPacket(&'a OutUdpPacket),
	SendPacket(&'a OutPacket),
}

/// An item that originates from a tsproto raw event stream.
///
/// The disconnected event is signaled by returning `None` from the stream.
#[derive(Debug)]
pub enum StreamItem {
	Command(InCommandBuf),
	Audio(InAudioBuf),
	C2SInit(InC2SInitBuf),
	S2CInit(InS2CInitBuf),
	/// All packets with an id less or equal to this id were acknowledged.
	AckPacket(PacketId),
	/// The network statistics were updated.
	NetworkStatsUpdated,
	Error(Error),
}

type EventListener = Box<dyn for<'a> Fn(&'a Event<'a>) + Send>;

/// Represents a currently alive connection.
pub struct Connection {
	pub is_client: bool,
	pub span: Span,
	/// The parameters of this connection, if it is already established.
	pub params: Option<ConnectedParams>,
	/// The address of the other side, where packets are coming from and going
	/// to.
	pub address: SocketAddr,

	pub resender: Resender,
	pub codec: PacketCodec,
	pub udp_socket: Box<dyn Socket + Send>,
	udp_buffer: Vec<u8>,

	/// A buffer of packets that should be returned from the stream.
	///
	/// If a new udp packet is received and we already received the following
	/// ids, we can get multiple packets back at once. As we can only return one
	/// from the stream, the rest is stored here.
	pub(crate) stream_items: VecDeque<StreamItem>,

	/// The queue of non-command packets that should be sent.
	///
	/// These packets are not influenced by congestion control.
	/// If it gets too long, we don't poll from the `udp_socket` anymore.
	acks_to_send: VecDeque<OutUdpPacket>,

	pub event_listeners: Vec<EventListener>,
}

impl Socket for UdpSocket {
	fn poll_recv_from(&self, cx: &mut Context, buf: &mut ReadBuf) -> Poll<io::Result<SocketAddr>> {
		self.poll_recv_from(cx, buf)
	}

	fn poll_send_to(
		&self, cx: &mut Context, buf: &[u8], target: SocketAddr,
	) -> Poll<io::Result<usize>> {
		self.poll_send_to(cx, buf, target)
	}

	fn local_addr(&self) -> io::Result<SocketAddr> { self.local_addr() }
}

impl Default for CachedKey {
	fn default() -> Self {
		CachedKey { generation_id: u32::MAX, key: [0; 16].into(), nonce: [0; 16].into() }
	}
}

impl ConnectedParams {
	/// Fills the parameters for a connection with their default state.
	pub fn new(public_key: EccKeyPubP256, shared_iv: [u8; 64], shared_mac: [u8; 8]) -> Self {
		Self {
			c_id: 0,
			voice_encryption: true,
			public_key,
			shared_iv,
			shared_mac,
			key_cache: Default::default(),
		}
	}
}

impl Connection {
	pub fn new(is_client: bool, address: SocketAddr, udp_socket: Box<dyn Socket + Send>) -> Self {
		let span = info_span!("connection", local_addr = %udp_socket.local_addr().unwrap(),
			remote_addr = %address);

		let mut res = Self {
			is_client,
			span,
			params: None,
			address,
			resender: Default::default(),
			codec: Default::default(),
			udp_socket,
			udp_buffer: Default::default(),

			stream_items: Default::default(),
			acks_to_send: Default::default(),
			event_listeners: Default::default(),
		};
		if is_client {
			// The first command is sent as part of the C2SInit::Init4 packet
			// so it does not get registered automatically.
			res.codec.outgoing_p_ids[PacketType::Command.to_usize().unwrap()] =
				PartialPacketId { generation_id: 0, packet_id: 1 };
		} else {
			res.codec.incoming_p_ids[PacketType::Command.to_usize().unwrap()] =
				PartialPacketId { generation_id: 0, packet_id: 1 };
		}
		res
	}

	/// Check if a given id is in the receive window.
	///
	/// Returns
	/// 1. If the packet id is inside the receive window
	/// 1. The generation of the packet
	/// 1. The minimum accepted packet id
	/// 1. The maximum accepted packet id
	pub(crate) fn in_receive_window(&self, p_type: PacketType, p_id: u16) -> (bool, u32, u16, u16) {
		if p_type == PacketType::Init {
			return (true, 0, 0, 0);
		}
		let type_i = p_type.to_usize().unwrap();
		// Receive window is the next half of ids
		let cur_next = self.codec.incoming_p_ids[type_i].packet_id;
		let (limit, next_gen) = cur_next.overflowing_add(u16::MAX / 2);
		let gen = self.codec.incoming_p_ids[type_i].generation_id;
		let in_recv_win = (!next_gen && p_id >= cur_next && p_id < limit)
			|| (next_gen && (p_id >= cur_next || p_id < limit));
		let gen_id = if in_recv_win {
			if next_gen && p_id < limit { gen + 1 } else { gen }
		} else if p_id < cur_next {
			gen
		} else {
			gen - 1
		};

		(in_recv_win, gen_id, cur_next, limit)
	}

	pub fn send_event(&self, event: &Event) {
		for l in &self.event_listeners {
			l(event)
		}
	}

	pub fn hand_back_buffer(&mut self, buffer: Vec<u8>) {
		if self.udp_buffer.capacity() < MAX_UDP_PACKET_LENGTH
			&& buffer.capacity() >= MAX_UDP_PACKET_LENGTH
		{
			self.udp_buffer = buffer;
		}
	}

	fn poll_send_acks(&mut self, cx: &mut Context) -> Result<()> {
		// Poll acks_to_send
		while let Some(packet) = self.acks_to_send.front() {
			match self.poll_send_udp_packet(cx, packet) {
				Poll::Ready(Ok(())) => {
					self.resender.handle_loss_outgoing(packet);
				}
				Poll::Ready(Err(e)) => return Err(e),
				Poll::Pending => break,
			}
			self.acks_to_send.pop_front();
		}
		Ok(())
	}

	fn poll_incoming_udp_packet(&mut self, cx: &mut Context) -> Poll<Result<StreamItem>> {
		if self.acks_to_send.len() >= UDP_SINK_CAPACITY {
			return Poll::Pending;
		}

		loop {
			// Poll udp_socket
			if self.udp_buffer.len() != MAX_UDP_PACKET_LENGTH {
				self.udp_buffer.resize(MAX_UDP_PACKET_LENGTH, 0);
			}

			let mut read_buf = ReadBuf::new(&mut self.udp_buffer);
			match self.udp_socket.poll_recv_from(cx, &mut read_buf) {
				Poll::Ready(Ok(addr)) => {
					let size = read_buf.filled().len();
					let mut udp_buffer = mem::take(&mut self.udp_buffer);
					udp_buffer.truncate(size);
					match self.handle_udp_packet(cx, udp_buffer, addr) {
						Ok(()) => {
							if let Some(item) = self.stream_items.pop_front() {
								return Poll::Ready(Ok(item));
							}
						}
						Err(e) => {
							return Poll::Ready(Err(e));
						}
					}
				}
				// Udp socket closed
				Poll::Ready(Err(e)) => return Poll::Ready(Err(Error::Network(e))),
				Poll::Pending => return Poll::Pending,
			}
		}
	}

	fn handle_udp_packet(
		&mut self, cx: &mut Context, udp_buffer: Vec<u8>, addr: SocketAddr,
	) -> Result<()> {
		let _span = self.span.clone().entered();
		if addr != self.address {
			self.stream_items.push_back(StreamItem::Error(Error::WrongAddress));
			return Ok(());
		}

		let dir = if self.is_client { Direction::S2C } else { Direction::C2S };
		let packet = InUdpPacket(match InPacket::try_new(dir, &udp_buffer) {
			Ok(r) => r,
			Err(e) => {
				self.stream_items.push_back(StreamItem::Error(Error::PacketParse("udp", e)));
				return Ok(());
			}
		});
		let event = Event::ReceiveUdpPacket(&packet);
		self.send_event(&event);

		self.resender.received_packet();
		PacketCodec::handle_udp_packet(self, cx, udp_buffer)?;

		Ok(())
	}

	/// Try to send an ack packet.
	///
	/// If it does not work, add it to the ack queue.
	pub(crate) fn send_ack_packet(&mut self, cx: &mut Context, packet: OutPacket) -> Result<()> {
		self.send_event(&Event::SendPacket(&packet));
		let mut udp_packets = PacketCodec::encode_packet(self, packet)?;
		assert_eq!(
			udp_packets.len(),
			1,
			"Encoding an ack packet should only yield a single packet"
		);
		let packet = udp_packets.pop().unwrap();

		match self.poll_send_udp_packet(cx, &packet) {
			Poll::Ready(r) => {
				if r.is_ok() {
					self.resender.handle_loss_outgoing(&packet);
				}
				r
			}
			Poll::Pending => {
				self.acks_to_send.push_back(packet);
				Ok(())
			}
		}
	}

	/// Add a packet to the send queue.
	///
	/// This function buffers indefinitely, to prevent using a large amount of
	/// memory, check `is_send_queue_full` first and only send a packet if this
	/// function returns `false`.
	///
	/// When the `PacketId` which is returned by this function is acknowledged,
	/// the packet was successfully received by the other side of the
	/// connection.
	pub fn send_packet(&mut self, packet: OutPacket) -> Result<PacketId> {
		self.send_event(&Event::SendPacket(&packet));
		let udp_packets = PacketCodec::encode_packet(self, packet)?;

		let id = udp_packets.last().unwrap().into();
		for p in udp_packets {
			self.send_udp_packet(p);
		}
		Ok(id)
	}

	/// Add an udp packet to the send queue.
	pub fn send_udp_packet(&mut self, packet: OutUdpPacket) {
		let _span = self.span.clone().entered();
		match packet.packet_type() {
			PacketType::Init | PacketType::Command | PacketType::CommandLow => {
				Resender::send_packet(self, packet);
			}
			_ => self.acks_to_send.push_back(packet),
		}
	}

	pub fn poll_send_udp_packet(
		&self, cx: &mut Context, packet: &OutUdpPacket,
	) -> Poll<Result<()>> {
		Self::static_poll_send_udp_packet(
			&*self.udp_socket,
			self.address,
			&self.event_listeners,
			cx,
			packet,
		)
	}

	/// Remember to add the size of the sent packet to the stats in the resender.
	pub fn static_poll_send_udp_packet(
		udp_socket: &dyn Socket, address: SocketAddr, event_listeners: &[EventListener],
		cx: &mut Context, packet: &OutUdpPacket,
	) -> Poll<Result<()>> {
		let data = packet.data().data();
		match udp_socket.poll_send_to(cx, data, address).map_err(Error::Network)? {
			Poll::Pending => Poll::Pending,
			Poll::Ready(size) => {
				let event = Event::SendUdpPacket(packet);
				for l in event_listeners {
					l(&event)
				}

				if size != data.len() {
					Poll::Ready(Err(Error::Network(std::io::Error::new(
						std::io::ErrorKind::Other,
						"Failed to send whole udp packet",
					))))
				} else {
					Poll::Ready(Ok(()))
				}
			}
		}
	}

	pub fn is_send_queue_full(&self) -> bool { self.resender.is_full() }
	pub fn is_send_queue_empty(&self) -> bool { self.resender.is_empty() }
}

/// Pull for events.
///
/// `Ok(StreamItem::Error)` is recoverable, `Err()` is not.
///
/// Polling does a few things in round robin fashion:
/// 1. Check for new udp packets
/// 2. Use the resender to resend packets if necessary
/// 3. Use the resender to send ping packets if necessary
impl Stream for Connection {
	type Item = Result<StreamItem>;
	fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<Self::Item>> {
		let _span = self.span.clone().entered();
		if let Err(e) = self.poll_send_acks(cx) {
			return Poll::Ready(Some(Err(e)));
		}

		if self.resender.get_state() == ResenderState::Disconnected {
			// Send all ack packets and return `None` afterwards
			if self.acks_to_send.is_empty() {
				return Poll::Ready(None);
			}
		}

		// Use the resender to resend packes
		match Resender::poll_resend(&mut self, cx) {
			Ok(()) => {}
			Err(e) => return Poll::Ready(Some(Err(e))),
		}

		// Use the resender to send pings
		match Resender::poll_ping(&mut self, cx) {
			Ok(()) => {}
			Err(e) => return Poll::Ready(Some(Err(e))),
		}

		// Return existing stream_items
		if let Some(item) = self.stream_items.pop_front() {
			return Poll::Ready(Some(Ok(item)));
		}

		// Check for new udp packets
		match self.poll_incoming_udp_packet(cx) {
			Poll::Ready(r) => return Poll::Ready(Some(r)),
			Poll::Pending => {}
		}

		Poll::Pending
	}
}
