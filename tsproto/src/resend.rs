use std::cmp::{Ord, Ordering};
use std::collections::BinaryHeap;
use std::convert::From;
use std::hash::{Hash, Hasher};
use std::mem;
use std::pin::Pin;
use std::task::{Context, Poll};

use anyhow::bail;
use futures::prelude::*;
use slog::warn;
use tokio::sync::oneshot;
use tokio::time::{Delay, Duration, Instant};
use tsproto_packets::packets::*;

use crate::{Result, UDP_SINK_CAPACITY};
use crate::connection::Connection;

// TODO implement fast retransmit: 2 Acks received but earlier packet not acked -> retransmit

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
	/// The connection is starting
	Connecting,
	/// The handshake is completed, this is the normal operation mode
	Connected,
	/// The connection is tearing down
	Disconnecting,
}

/// For each connection a resender is created, which is responsible for sending
/// command packets and ensure that they are delivered.
pub trait Resender {
	/// Called for a received ack packet.
	///
	/// The packet type must be `Command` or `CommandLow`.
	fn ack_packet(&mut self, p_type: PacketType, p_id: u16);

	/// This method informs the resender of state changes of the connection.
	fn change_state(&mut self, event: ResenderState);
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
	pub packet: OutUdpPacket,
	/// Notify this sender when the packet was acknowledged.
	pub notify: oneshot::Sender<()>,
}

/// An implementation of a `Resender` that is provided by this library.
#[derive(Debug)]
pub struct DefaultResender {
	to_send: BinaryHeap<SendRecord>,
	config: ResendConfig,
	state: ResenderState,

	// Congestion control
	/// The maximum send window before the last reduction.
	w_max: u16,
	/// The amount of packets that were sent since the last loss.
	packet_count: u16,

	/// The future to wake us up when the next packet should be resent.
	timeout: Delay,
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

impl PartialEq for SendRecord {
	fn eq(&self, other: &Self) -> bool { self.cmp(other) == Ordering::Equal }
}
impl Eq for SendRecord {}

impl PartialOrd for SendRecord {
	fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
		Some(self.cmp(other))
	}
}

impl SendRecord {
	fn get_id(&self) -> (PacketType, u32, u16) {
		(self.packet.packet_type(), self.packet.generation_id(),
			self.packet.packet_id())
	}
}

impl Ord for SendRecord {
	fn cmp(&self, other: &Self) -> Ordering {
		// If the packet was not already sent, it is more important
		if self.tries == 0 {
			if other.tries == 0 {
				self.packet.generation_id()
					.cmp(&other.packet.generation_id())
					.reverse()
					.then_with(|| self.packet.packet_id().cmp(&other.packet.packet_id()).reverse())
			} else {
				Ordering::Greater
			}
		} else if other.tries == 0 {
			Ordering::Less
		} else {
			// The smallest time is the most important time
			self.last.cmp(&other.last).reverse().then_with(||
				// Else, the lower packet id is more important
				self.packet.generation_id().cmp(&other.packet.generation_id()).reverse().then_with(||
					self.packet.packet_id().cmp(&other.packet.packet_id()).reverse()))
		}
	}
}

impl Hash for SendRecord {
	fn hash<H: Hasher>(&self, state: &mut H) { self.get_id().hash(state); }
}

impl Default for DefaultResender {
	fn default() -> Self {
		Self {
			to_send: Default::default(),
			config: Default::default(),
			state: ResenderState::Connecting,

			w_max: UDP_SINK_CAPACITY as u16,
			packet_count: 0,

			timeout: tokio::time::delay_for(std::time::Duration::from_secs(1)),
		}
	}
}

impl Resender for DefaultResender {
	fn ack_packet(&mut self, p_type: PacketType, p_id: u16) {
		let rec = if let Some(rec) = self.to_send.peek() {
			if rec.packet.packet_type() == p_type && rec.packet.packet_id() == p_id {
				// Optimized to remove the first element
				self.to_send.pop()
			} else {
				// Convert to vector to remove the element
				let tmp = mem::replace(&mut self.to_send, BinaryHeap::new());
				let mut v = tmp.into_vec();
				let rec = v.iter().position(|rec| {
					rec.packet.packet_type() == p_type && rec.packet.packet_id() == p_id
				}).map(|i| v.remove(i));
				mem::replace(&mut self.to_send, v.into());
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
				self.update_srtt(now - rec.sent);
			}
			// Ignore if the receiver was dropped
			let _ = rec.notify.send(());
		}
	}

	fn change_state(&mut self, state: ResenderState) {
		self.state = state;
	}
}

impl DefaultResender {
	fn get_timeout(&self) -> Duration {
		match self.state {
			ResenderState::Connecting => self.config.connecting_timeout,
			ResenderState::Disconnecting => self.config.disconnect_timeout,
			ResenderState::Connected => self.config.normal_timeout,
		}
	}

	pub fn get_state(&self) -> ResenderState { self.state }

	/// The amount of packets that can be in-flight currently.
	///
	/// The CUBIC congestion control window.
	fn get_window(&self) -> u16 {
	 	let res = C * (self.packet_count as f32
	 			- (self.w_max as f32 * BETA / C).powf(1.0 / 3.0))
	 		.powf(3.0) + self.w_max as f32;
	 	if res > u16::max_value() as f32 {
	 		u16::max_value()
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

	pub async fn send_packet(con: &mut Connection, packet: OutUdpPacket) -> Result<oneshot::Receiver<()>> {
		let (notify, recv) = oneshot::channel();
		let mut rec = SendRecord {
			sent: Instant::now(),
			last: Instant::now(),
			tries: 0,
			packet,
			notify,
		};

		if con.resender.to_send.len() < usize::from(con.resender.get_window()) {
			// Send the packet right now
			future::poll_fn(|cx| con.poll_send_udp_packet(cx, &rec.packet)).await?;
			rec.tries = 1;
		}
		con.resender.to_send.push(rec);
		Ok(recv)
	}

	/// Returns an error if the timeout is exceeded and the connection is
	/// considered dead or another unrecoverable error occurs.
	pub fn poll_resend(con: &mut Connection, cx: &mut Context) -> Result<()> {
		let now = Instant::now();

		// Check if there are packets to send.
		let mut rto: Duration;
		let mut last_threshold;
		let mut packet_loss = false;
		let timeout = con.resender.get_timeout();
		let max_rto = timeout / 2;
		while let Some(rec) = {
			// Handle congestion window when the resender is not borrowed
			if packet_loss {
				con.resender.w_max = con.resender.get_window();
				con.resender.packet_count = 0;
			} else {
				con.resender.packet_count += 1;
			}

			// Retransmission timeout
			rto = con.resender.config.srtt + con.resender.config.srtt_dev * 4;
			if rto > max_rto {
				rto = max_rto;
			}
			last_threshold = now - rto;

			if let Some(rec) = &mut con.resender.to_send.peek_mut() {
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
			}
		} {
			if now - rec.sent > timeout {
				bail!("Connection timed out");
			}

			// Try to send this packet
			match Connection::static_poll_send_udp_packet(&con.udp_socket, &con.address, cx, &rec.packet) {
				Poll::Pending => break,
				Poll::Ready(Err(e)) => return Err(e),
				Poll::Ready(Ok(())) => {
					// Successfully started sending the packet, now schedule the
					// next send time for this packet and enqueue it.
					// Double srtt on packet loss
					if rec.tries != 0 {
						con.resender.config.srtt = con.resender.config.srtt * 2;
						if con.resender.config.srtt > max_rto {
							con.resender.config.srtt = max_rto;
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
							"id" => ?rec.get_id(),
							"tries" => rec.tries,
							"last" => ?rec.last,
							"to" => to_s,
							"srtt" => ?con.resender.config.srtt,
							"srtt_dev" => ?con.resender.config.srtt_dev,
							"rto" => ?rto,
							"threshold" => ?last_threshold,
						);
					}
				}
			}
		}

		Ok(())
	}
}

impl Default for ResendConfig {
	fn default() -> Self {
		Self {
			connecting_timeout: Duration::from_secs(5),
			normal_timeout: Duration::from_secs(30),
			disconnect_timeout: Duration::from_secs(5),

			srtt: Duration::from_millis(2500),
			srtt_dev: Duration::from_millis(0),
		}
	}
}
