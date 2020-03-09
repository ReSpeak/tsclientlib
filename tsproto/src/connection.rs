use std::collections::VecDeque;
use std::mem;
use std::net::SocketAddr;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::u16;

use aes::block_cipher_trait::generic_array::typenum::consts::U16;
use aes::block_cipher_trait::generic_array::GenericArray;
use anyhow::{bail, format_err, Context as _};
use futures::prelude::*;
use futures::stream::FuturesUnordered;
use num_traits::ToPrimitive;
use slog::{o, Logger};
use tokio::net::UdpSocket;
use tsproto_packets::HexSlice;
use tsproto_packets::packets::*;

use crate::{Error, MAX_UDP_PACKET_LENGTH, Result, UDP_SINK_CAPACITY};
use crate::crypto::EccKeyPubP256;
use crate::packet_codec::PacketCodec;
use crate::resend::{DefaultResender, ResenderState};

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
	Error(Error),
}

type EventListener = Box<dyn for<'a> Fn(&'a Event<'a>) -> () + Send>;

/// Represents a currently alive connection.
pub struct Connection {
	pub is_client: bool,
	pub logger: Logger,
	/// The parameters of this connection, if it is already established.
	pub params: Option<ConnectedParams>,
	/// The adress of the other side, where packets are coming from and going
	/// to.
	pub address: SocketAddr,

	pub resender: DefaultResender,
	pub codec: PacketCodec,
	pub udp_socket: UdpSocket,
	udp_buffer: Vec<u8>,

	/// Used in the stream implementation.
	next_poll: u8,

	/// A buffer of packets that should be returned from the stream.
	///
	/// If a new udp packet is received and we already received the following
	/// ids, we can get multiple packets back at once. As we can only return one
	/// from the stream, the rest is stored here.
	pub(crate) stream_items: VecDeque<StreamItem>,

	/// The internal queue of ack packets that should be sent.
	///
	/// If it gets too long, polling from the udp socket is blocked.
	acks_to_send: VecDeque<OutUdpPacket>,

	pub event_listeners: Vec<EventListener>,
}

impl Default for CachedKey {
	fn default() -> Self {
		CachedKey {
			generation_id: u32::max_value(),
			key: [0; 16].into(),
			nonce: [0; 16].into(),
		}
	}
}

impl ConnectedParams {
	/// Fills the parameters for a connection with their default state.
	pub fn new(
		public_key: EccKeyPubP256,
		shared_iv: [u8; 64],
		shared_mac: [u8; 8],
	) -> Self
	{
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
	pub fn new(
		is_client: bool,
		logger: Logger,
		address: SocketAddr,
		udp_socket: UdpSocket,
	) -> Self
	{
		let logger = logger.new(o!("addr" => address.to_string()));

		let mut res = Self {
			is_client,
			logger,
			params: None,
			address,
			resender: Default::default(),
			codec: Default::default(),
			udp_socket,
			udp_buffer:  Default::default(),
			next_poll: Default::default(),

			stream_items: Default::default(),
			acks_to_send: Default::default(),
			event_listeners: Default::default(),
		};
		if is_client {
			// The first command is sent as part of the C2SInit::Init4 packet
			// so it does not get registered automatically.
			res.codec.outgoing_p_ids[PacketType::Command.to_usize().unwrap()] =
				(0, 1);
		} else {
			res.codec.incoming_p_ids[PacketType::Command.to_usize().unwrap()] =
				(0, 1);
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
	pub(crate) fn in_receive_window(
		&self,
		p_type: PacketType,
		p_id: u16,
	) -> (bool, u32, u16, u16)
	{
		if p_type == PacketType::Init {
			return (true, 0, 0, 0);
		}
		let type_i = p_type.to_usize().unwrap();
		// Receive window is the next half of ids
		let cur_next = self.codec.incoming_p_ids[type_i].1;
		let (limit, next_gen) = cur_next.overflowing_add(u16::MAX / 2);
		let gen = self.codec.incoming_p_ids[type_i].0;
		let in_recv_win = (!next_gen && p_id >= cur_next && p_id < limit)
			|| (next_gen && (p_id >= cur_next || p_id < limit));
		let gen_id = if in_recv_win {
			if next_gen && p_id < limit { gen + 1 } else { gen }
		} else {
			if p_id < cur_next { gen } else { gen - 1 }
		};

		(
			in_recv_win,
			gen_id,
			cur_next,
			limit,
		)
	}

	pub fn send_event(&self, event: &Event) {
		for l in &self.event_listeners {
			l(event)
		}
	}

	pub fn hand_back_buffer(&mut self, buffer: Vec<u8>) {
		if self.udp_buffer.capacity() < MAX_UDP_PACKET_LENGTH
			&& buffer.capacity() >= MAX_UDP_PACKET_LENGTH {
			self.udp_buffer = buffer;
		}
	}

	fn poll_incoming_udp_packet(&mut self, cx: &mut Context) -> Poll<Result<StreamItem>> {
		// Poll acks_to_send
		while let Some(packet) = self.acks_to_send.front() {
			match self.poll_send_udp_packet(cx, packet) {
				Poll::Ready(Ok(())) => {}
				Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
				Poll::Pending => break,
			}
			self.acks_to_send.pop_front();
		}
		if self.acks_to_send.len() >= UDP_SINK_CAPACITY
			|| self.resender.get_state() == ResenderState::Disconnecting {
			return Poll::Pending;
		}

		// Poll stream_items
		if let Some(item) = self.stream_items.pop_front() {
			return Poll::Ready(Ok(item));
		}

		loop {
			// Poll udp_socket
			if self.udp_buffer.len() != MAX_UDP_PACKET_LENGTH {
				self.udp_buffer.resize(MAX_UDP_PACKET_LENGTH, 0);
			}

			match self.udp_socket.poll_recv_from(cx, &mut self.udp_buffer) {
				Poll::Ready(Ok((size, addr))) => {
					let mut udp_buffer = mem::replace(&mut self.udp_buffer, Vec::new());
					udp_buffer.truncate(size);
					match self.handle_udp_packet(cx, udp_buffer, addr) {
						Ok(()) => if let Some(item) = self.stream_items.pop_front() {
							return Poll::Ready(Ok(item));
						}
						Err(e) => {
							return Poll::Ready(Ok(StreamItem::Error(e)));
						}
					}
				}
				// Udp socket closed
				Poll::Ready(Err(e)) => return Poll::Ready(Err(e.into())),
				Poll::Pending => return Poll::Pending,
			}
		}
	}

	fn poll_resender_ping(&mut self, _cx: &mut Context) -> Poll<Option<Result<StreamItem>>> {
		// Return None if disconnected
		// TODO
		Poll::Pending
	}

	fn handle_udp_packet(&mut self, cx: &mut Context, udp_buffer: Vec<u8>, addr: SocketAddr) -> Result<()> {
		if addr != self.address {
			bail!("Received UDP packet from wrong address");
		}

		let dir = if self.is_client { Direction::S2C } else { Direction::C2S };
		let packet = InUdpPacket(InPacket::try_new(dir, &udp_buffer)
			.with_context(|| format!("Buffer {:?}", HexSlice(&udp_buffer)))?);
		let event = Event::ReceiveUdpPacket(&packet);
		self.send_event(&event);

		PacketCodec::handle_udp_packet(self, cx, udp_buffer)?;

		Ok(())
	}

	/// Try to send an ack packet.
	///
	/// If it does not work, add it to the ack queue.
	pub(crate) fn send_ack_packet(&mut self, cx: &mut Context, packet: OutPacket) -> Result<()> {
		self.send_event(&Event::SendPacket(&packet));
		let mut udp_packets = PacketCodec::encode_packet(self, packet)?;
		assert_eq!(udp_packets.len(), 1, "Encoding an ack packet should only yield a single packet");
		let packet = udp_packets.pop().unwrap();

		match self.poll_send_udp_packet(cx, &packet) {
			Poll::Ready(r) => r,
			Poll::Pending => {
				self.acks_to_send.push_back(packet);
				Ok(())
			}
		}
	}

	/// Send a packet. The `send_packet` function will resolve to a future when
	/// the packet has been sent.
	///
	/// If the packet has an acknowledgement (ack or pong), the returned future
	/// will resolve when it is received. Otherwise it will resolve instantly.
	pub async fn send_packet(&mut self, packet: OutPacket) -> impl Future<Output=Result<()>> {
		self.send_event(&Event::SendPacket(&packet));
		let udp_packets = match PacketCodec::encode_packet(self, packet) {
			Ok(r) => r,
			Err(e) => return future::err(e).left_future(),
		};

		let fut = FuturesUnordered::new();
		for p in udp_packets {
			fut.push(self.send_udp_packet(p).await);
		}
		fut.try_for_each_concurrent(None, |_| future::ok(())).right_future()
	}

	// Non-command packets are not influenced by congestion control
	/// Send a udp packet.
	///
	/// If the packet has an acknowledgement (ack or pong), the future will
	/// resolve when it is received.
	pub async fn send_udp_packet(&mut self, packet: OutUdpPacket) -> impl Future<Output=Result<()>> {
		match packet.packet_type() {
			PacketType::Init | PacketType::Command | PacketType::CommandLow => {
				match DefaultResender::send_packet(self, packet).await {
					Ok(r) => r.map_err(|e| e.into()).left_future(),
					Err(e) => future::err(e).right_future(),
				}
			}
			_ => {
				future::ready(future::poll_fn(|cx| self.poll_send_udp_packet(cx, &packet)).await).right_future()
			}
		}
	}

	pub fn poll_send_udp_packet(&self, cx: &mut Context, packet: &OutUdpPacket) -> Poll<Result<()>> {
		Self::static_poll_send_udp_packet(&self.udp_socket, &self.address, &self.event_listeners, cx, packet)
	}

	pub fn static_poll_send_udp_packet(
		udp_socket: &UdpSocket,
		address: &SocketAddr,
		event_listeners: &[EventListener],
		cx: &mut Context,
		packet: &OutUdpPacket,
	) -> Poll<Result<()>>
	{
		let data = packet.data().data();
		match udp_socket.poll_send_to(cx, data, address)? {
			Poll::Pending => Poll::Pending,
			Poll::Ready(size) => {
				let event = Event::SendUdpPacket(&packet);
				for l in event_listeners {
					l(&event)
				}

				if size != data.len() {
					Poll::Ready(Err(format_err!("Failed to send whole udp packet")))
				} else {
					Poll::Ready(Ok(()))
				}
			}
		}
	}
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
		const COUNT: u8 = 3;
		if self.resender.get_state() == ResenderState::Disconnecting {
			// Send all ack packets and return `None` afterwards
			let _ = self.poll_incoming_udp_packet(cx);
			if self.acks_to_send.is_empty() {
				return Poll::Ready(None);
			}
		}

		for _ in 0..COUNT {
			self.next_poll = (self.next_poll + 1) % COUNT;
			match self.next_poll {
				0 => {
					// Check for new udp packets
					match self.poll_incoming_udp_packet(cx) {
						Poll::Pending => {}
						Poll::Ready(r) => return Poll::Ready(Some(r)),
					}
				}
				1 => {
					// Use the resender to resend packes
					match DefaultResender::poll_resend(&mut *self, cx) {
						Ok(()) => {}
						Err(e) => return Poll::Ready(Some(Err(e))),
					}
				}
				2 => {
					// Use the resender to send pings
					match self.poll_resender_ping(cx) {
						Poll::Pending => {}
						r => return r,
					}
				}
				_ => unreachable!(),
			}
		}

		Poll::Pending
	}
}
