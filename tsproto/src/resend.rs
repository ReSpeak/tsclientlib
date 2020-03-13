use std::cmp::{Ord, Ordering};
use std::collections::{BinaryHeap, BTreeSet};
use std::convert::From;
use std::hash::{Hash, Hasher};
use std::mem;
use std::pin::Pin;
use std::task::{Context, Poll};

use anyhow::bail;
use futures::prelude::*;
use slog::{info, warn, Logger};
use tokio::time::{Delay, Duration, Instant};
use tsproto_packets::packets::*;

use crate::{Result, UDP_SINK_CAPACITY};
use crate::connection::{Connection, StreamItem};

// TODO implement fast retransmit: 2 Acks received but earlier packet not acked -> retransmit
// TODO implement slow start and redo slow start after x packet losses

// Use cubic for congestion control: https://en.wikipedia.org/wiki/CUBIC_TCP
// But scaling with number of sent packets instead of time because we might not
// send packets that often.

/// Congestion windows gets down to 0.3*w_max for BETA=0.7
const BETA: f32 = 0.7;
/// Increase over w_max after roughly 5 packets (C=0.2 needs seven packets).
const C: f32 = 0.5;

/// Events to inform a resender of the current state of a connection.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum ResenderState {
	/// The connection is starting, reduce the timeout time.
	Connecting,
	/// The handshake is completed, this is the normal operation mode.
	Connected,
	/// The connection is tearing down, reduce the timeout time.
	Disconnecting,
	/// The connection is gone, we only send ack packets.
	Disconnected,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct PacketId {
	pub packet_type: PacketType,
	pub generation_id: u32,
	pub packet_id: u16,
}

/// A record of a packet that can be resent.
#[derive(Debug)]
struct SendRecord {
	/// When this packet was originally sent.
	pub sent: Instant,
	/// The last time when the packet was sent.
	pub last: Instant,
	/// How often the packet was already resent.
	pub tries: usize,
	pub id: PacketId,
	pub packet: OutUdpPacket,
}

/// Resend command and init packets until the other side acknowledges them.
#[derive(Debug)]
pub struct Resender {
	/// Send queue ordered by when a packet has to be sent.
	///
	/// The maximum in this queue is the next packet that should be resent.
	to_send: BinaryHeap<SendRecord>,
	/// Send queue ordered by packet id.
	///
	/// There is one queue per packet type: `Init`, `Command` and `CommandLow`.
	to_send_ordered: [BTreeSet<PacketId>; 3],
	config: ResendConfig,
	state: ResenderState,

	// Congestion control
	/// The maximum send window before the last reduction.
	w_max: u16,
	/// The amount of packets that were sent since the last loss.
	packet_count: u16,

	/// When the last packet was added to the send queue or received.
	///
	/// This is used to decide when to send ping packets.
	last_receive: Instant,
	/// When the last packet was added to the send queue.
	///
	/// This is used to handle timeouts when disconnecting.
	last_send: Instant,

	/// The future to wake us up when the next packet should be resent.
	timeout: Delay,
	/// The timer used for sending ping packets.
	ping_timeout: Delay,
	/// The timer used for disconnecting the connection.
	state_timeout: Delay,
}

#[derive(Clone, Debug)]
pub struct ResendConfig {
	// Close the connection after no packet is received for this duration.
	pub connecting_timeout: Duration,
	pub normal_timeout: Duration,
	pub disconnect_timeout: Duration,

	/// Start value for the Smoothed Round Trip Time.
	pub srtt: Duration,
	/// Start value for the deviation of the srtt.
	pub srtt_dev: Duration,
}

impl Ord for PacketId {
	fn cmp(&self, other: &Self) -> Ordering {
		self.generation_id.cmp(&other.generation_id).then_with(||
			self.packet_id.cmp(&other.packet_id))
	}
}

impl PartialOrd for PacketId {
	fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
		Some(self.cmp(other))
	}
}

impl From<&OutUdpPacket> for PacketId {
	fn from(packet: &OutUdpPacket) -> Self {
		Self {
			packet_type: packet.packet_type(),
			generation_id: packet.generation_id(),
			packet_id: packet.packet_id(),
		}
	}
}

impl Ord for SendRecord {
	fn cmp(&self, other: &Self) -> Ordering {
		// If the packet was not already sent, it is more important
		if self.tries == 0 {
			if other.tries == 0 {
				self.id.cmp(&other.id).reverse()
			} else {
				Ordering::Greater
			}
		} else if other.tries == 0 {
			Ordering::Less
		} else {
			// The smallest time is the most important time
			self.last.cmp(&other.last).reverse().then_with(||
				// Else, the lower packet id is more important
				self.id.cmp(&other.id).reverse())
		}
	}
}

impl PartialOrd for SendRecord {
	fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
		Some(self.cmp(other))
	}
}

impl PartialEq for SendRecord {
	fn eq(&self, other: &Self) -> bool {
		self.id.eq(&other.id)
	}
}
impl Eq for SendRecord {}

impl Hash for SendRecord {
	fn hash<H: Hasher>(&self, state: &mut H) { self.id.hash(state); }
}

impl Default for Resender {
	fn default() -> Self {
		Self {
			to_send: Default::default(),
			to_send_ordered: Default::default(),
			config: Default::default(),
			state: ResenderState::Connecting,

			w_max: UDP_SINK_CAPACITY as u16,
			packet_count: 0,

			timeout: tokio::time::delay_for(std::time::Duration::from_secs(1)),
			last_receive: Instant::now(),
			last_send: Instant::now(),
			ping_timeout: tokio::time::delay_for(std::time::Duration::from_secs(1)),
			state_timeout: tokio::time::delay_for(std::time::Duration::from_secs(1)),
		}
	}
}

impl Resender {
	fn packet_type_to_index(t: PacketType) -> usize {
		match t {
			PacketType::Init => 0,
			PacketType::Command => 1,
			PacketType::CommandLow => 2,
			_ => panic!("Resender cannot handle packet type {:?}", t),
		}
	}

	pub fn ack_packet(con: &mut Connection, p_type: PacketType, p_id: u16) {
		let rec = if let Some(rec) = con.resender.to_send.peek() {
			if rec.packet.packet_type() == p_type && rec.packet.packet_id() == p_id {
				// Optimized to remove the first element
				con.resender.to_send.pop()
			} else {
				// Convert to vector to remove the element
				let tmp = mem::replace(&mut con.resender.to_send, BinaryHeap::new());
				let mut v = tmp.into_vec();
				let rec = v.iter().position(|rec| {
					rec.packet.packet_type() == p_type && rec.packet.packet_id() == p_id
				}).map(|i| v.remove(i));
				mem::replace(&mut con.resender.to_send, v.into());
				rec
			}
		} else {
			// Do nothing if the heap is empty
			None
		};

		if let Some(rec) = rec {
			// Update srtt if the packet was not resent
			if rec.tries == 1 {
				let now = Instant::now();
				con.resender.update_srtt(now - rec.sent);
			}

			// Remove from ordered queue
			let queue = &mut con.resender.to_send_ordered[Self::packet_type_to_index(rec.id.packet_type)];
			let is_first = queue.iter().next() == Some(&rec.id);
			queue.remove(&rec.id);
			if is_first {
				con.stream_items.push_back(StreamItem::AckPacket(rec.id));
			}
		}
	}

	pub fn received_packet(&mut self) {
		self.last_receive = Instant::now();
	}

	fn get_timeout(&self) -> Duration {
		match self.state {
			ResenderState::Connecting => self.config.connecting_timeout,
			ResenderState::Disconnecting
			| ResenderState::Disconnected => self.config.disconnect_timeout,
			ResenderState::Connected => self.config.normal_timeout,
		}
	}

	/// Inform the resender of state changes of the connection.
	pub fn set_state(&mut self, logger: &Logger, state: ResenderState) {
		info!(logger, "Resender: Changed state"; "from" => ?self.state,
			"to" => ?state);
		self.state = state;

		self.last_send = Instant::now();
		self.state_timeout.reset(self.last_send + self.get_timeout());
	}

	pub fn get_state(&self) -> ResenderState { self.state }

	/// If the send queue is full.
	pub fn is_full(&self) -> bool {
		self.to_send.len() >= self.get_window() as usize
	}

	/// If the send queue is empty.
	pub fn is_empty(&self) -> bool {
		self.to_send.is_empty()
	}

	/// The amount of packets that can be in-flight currently.
	///
	/// The CUBIC congestion control window.
	fn get_window(&self) -> u16 {
		let res = C * (self.packet_count as f32
				- (self.w_max as f32 * BETA / C).powf(1.0 / 3.0))
			.powf(3.0) + self.w_max as f32;
		let max = u16::max_value() / 2;
		if res > max as f32 {
			max
		} else if res <= 1.0 {
			1
		} else {
			res as u16
		}
	}

	/// Add another duration to the stored smoothed rtt.
	fn update_srtt(&mut self, rtt: Duration) {
		let diff = if rtt > self.config.srtt { rtt - self.config.srtt } else { self.config.srtt - rtt };
		self.config.srtt_dev = self.config.srtt_dev * 3 / 4 + diff / 4;
		self.config.srtt = self.config.srtt * 7 / 8 + rtt / 8;
	}

	pub fn send_packet(con: &mut Connection, packet: OutUdpPacket) {
		con.resender.last_send = Instant::now();
		let rec = SendRecord {
			sent: Instant::now(),
			last: Instant::now(),
			tries: 0,
			id: (&packet).into(),
			packet,
		};

		let i = Self::packet_type_to_index(rec.id.packet_type);
		con.resender.to_send_ordered[i].insert(rec.id.clone());
		con.resender.to_send.push(rec);
	}

	/// Returns an error if the timeout is exceeded and the connection is
	/// considered dead or another unrecoverable error occurs.
	pub fn poll_resend(con: &mut Connection, cx: &mut Context) -> Result<()> {
		let now = Instant::now();
		let timeout = con.resender.get_timeout();
		let max_send_rto = Duration::from_secs(2);

		// Check if there are packets to send.
		let mut rto: Duration;
		let mut last_threshold;
		let mut packet_loss = false;
		let mut window;
		let mut done = 0;
		while let Some(mut rec) = {
			done += 1;
			// Handle congestion window when the resender is not borrowed
			if packet_loss {
				con.resender.w_max = con.resender.get_window();
				con.resender.packet_count = 0;
			} else {
				// TODO Depend on time instead
				con.resender.packet_count += 1;
			}
			window = con.resender.get_window();

			// Don't send more packets than our window size
			if done > window {
				return Ok(());
			}

			// Retransmission timeout
			rto = con.resender.config.srtt + con.resender.config.srtt_dev * 4;
			if rto > max_send_rto {
				rto = max_send_rto;
			}
			last_threshold = now - rto;

			let res = if let Some(rec) = con.resender.to_send.peek_mut() {
				// Check if we should resend this packet or not
				if rec.tries != 0 && rec.last > last_threshold {
					// Schedule next send
					let dur = rec.last - last_threshold;
					con.resender.timeout.reset(Instant::now() + dur);
					if let Poll::Ready(()) = Pin::new(&mut con.resender.timeout).poll(cx) {
						cx.waker().wake_by_ref();
					}
					return Ok(());
				}
				Some(rec)
			} else {
				None
			};

			if res.as_ref().map(|rec| now - rec.sent > timeout).unwrap_or_default() {
				drop(res);
				con.resender.to_send.clear();
				bail!("Connection timed out");
			}

			res
		} {
			// Try to send this packet
			match Connection::static_poll_send_udp_packet(&*con.udp_socket, &con.address, &con.event_listeners, cx, &rec.packet) {
				Poll::Pending => break,
				Poll::Ready(Err(e)) => return Err(e),
				Poll::Ready(Ok(())) => {
					// Successfully started sending the packet, now schedule the
					// next send time for this packet and enqueue it.
					// Double srtt on packet loss
					if rec.tries != 0 {
						con.resender.config.srtt = con.resender.config.srtt * 2;
						if con.resender.config.srtt > timeout {
							con.resender.config.srtt = timeout;
						}
						packet_loss = true;
					} else {
						packet_loss = false;
					}

					// Update record
					rec.last = now;
					rec.tries += 1;

					if rec.tries != 1 {
						let to_s = if con.is_client { "S" } else { "C" };
						warn!(con.logger, "Resend";
							"id" => ?rec.id,
							"tries" => rec.tries,
							"last" => ?rec.last,
							"to" => to_s,
							"srtt" => ?con.resender.config.srtt,
							"srtt_dev" => ?con.resender.config.srtt_dev,
							"rto" => ?rto,
							"threshold" => ?last_threshold,
							"send_window" => window,
						);
					}
				}
			}
		}

		Ok(())
	}

	/// Returns an error if the timeout is exceeded and the connection is
	/// considered dead or another unrecoverable error occurs.
	pub fn poll_ping(con: &mut Connection, cx: &mut Context) -> Result<()> {
		let now = Instant::now();
		let timeout = con.resender.get_timeout();

		if con.resender.state == ResenderState::Disconnecting {
			if now - con.resender.last_send >= timeout {
				bail!("Connection timed out");
			}

			con.resender.state_timeout.reset(con.resender.last_send + timeout);
			if let Poll::Ready(()) = Pin::new(&mut con.resender.state_timeout).poll(cx) {
				bail!("Connection timed out");
			}
		}

		// TODO Send ping packets if needed
		Ok(())
	}
}

impl Default for ResendConfig {
	fn default() -> Self {
		Self {
			connecting_timeout: Duration::from_secs(5),
			normal_timeout: Duration::from_secs(30),
			disconnect_timeout: Duration::from_secs(5),

			srtt: Duration::from_millis(500),
			srtt_dev: Duration::from_millis(0),
		}
	}
}
