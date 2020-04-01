//! Handle receiving audio.
//!
//! The [`AudioHandler`] collects all incoming audio packets and queues them per
//! client. It decodes the audio, handles out-of-order packets and missing
//! packets. It automatically adjusts the queue length based on the jitter of
//! incoming packets.
//!
//! [`AudioHandler`]: struct.AudioHandler.html

use std::collections::{HashMap, VecDeque};
use std::hash::Hash;

use anyhow::{bail, Result};
use audiopus::{packet, Channels, SampleRate};
use audiopus::coder::Decoder;
use slog::{debug, trace, warn, Logger};
use tsproto_packets::packets::{AudioData, CodecType, InAudioBuf};

use crate::ClientId;

const SAMPLE_RATE: SampleRate = SampleRate::Hz48000;
const CHANNELS: Channels = Channels::Stereo;
const CHANNEL_NUM: usize = 2;
/// If this amount of packets is lost consecutively, we assume the stream stopped.
const MAX_PACKET_LOSSES: usize = 3;
/// Store the buffer sizes for the last `LAST_BUFFER_SIZE_COUNT` packets.
const LAST_BUFFER_SIZE_COUNT: u16 = 512;
/// The amount of samples to maximally buffer. Equivalent to 0.5 s.
const MAX_BUFFER_SIZE: usize = 48_000 / 2;
/// Maximum number of packets in the queue.
const MAX_BUFFER_PACKETS: usize = 50;
/// Buffer for maximal 0.5 s without playing anything.
const MAX_BUFFER_TIME: usize = 48_000 / 2;
/// Duplicate or remove every `step` sample when speeding-up.
const SPEED_CHANGE_STEPS: usize = 100;

#[derive(Clone, Debug)]
struct SlidingWindowMinimum {
	/// How long a value stays in the sliding window.
	size: u16,
	/// This is a sliding window minimum, it contains
	/// `(insertion time, value)`.
	///
	/// When we insert a value, we can remove all bigger sample counts,
	/// thus the queue always stays sorted with the minimum at the front
	/// and the maximum at the back (latest entry).
	///
	/// Provides amortized O(1) minimum.
	/// Source: https://people.cs.uct.ac.za/~ksmith/articles/sliding_window_minimum.html#sliding-window-minimum-algorithm
	queue: VecDeque<(u16, usize)>,
	/// The current insertion time.
	cur_time: u16,
}

struct QueuePacket {
	packet: InAudioBuf,
	samples: usize,
	id: u16,
}

/// A queue for audio packets for one audio stream.
pub struct AudioQueue {
	decoder: Decoder,
	/// The id of the next packet that should be decoded.
	///
	/// Used to check for packet loss.
	next_id: u16,
	/// If the last packet was a whisper packet.
	whispering: bool,
	packet_buffer: VecDeque<QueuePacket>,
	/// Amount of samples in the `packet_buffer`.
	packet_buffer_samples: usize,
	/// Temporary buffer that contains the samples of one decoded packet.
	decoded_buffer: Vec<f32>,
	/// The current position in the `decoded_buffer`.
	decoded_pos: usize,
	/// The number of samples in the last packet.
	last_packet_samples: usize,
	/// The last `packet_loss_num` packet decodes were a loss.
	packet_loss_num: usize,
	/// The amount of samples to buffer until this queue is ready to play.
	buffering_samples: usize,
	/// The amount of samples in the buffer when a packet was decoded.
	///
	/// Used to expand or reduce the buffer.
	last_buffer_samples: SlidingWindowMinimum,
	/// Buffered for this duration.
	buffered_for_samples: usize,
}

/// Handles incoming audio, has one [`AudioQueue`] per sending client.
///
/// [`AudioQueue`]: struct.AudioQueue.html
pub struct AudioHandler<Id: Clone + Eq + Hash + PartialEq = ClientId> {
	logger: Logger,
	queues: HashMap<Id, AudioQueue>,
	talkers_changed: bool,
	/// Buffer this amount of samples for new queues before starting to play.
	///
	/// Updated when a new queue gets added.
	avg_buffer_samples: usize,
}

impl SlidingWindowMinimum {
	fn new(size: u16) -> Self {
		Self { size, queue: Default::default(), cur_time: 0 }
	}

	fn push(&mut self, value: usize) {
		while self.queue.back().map(|(_, s)| *s >= value).unwrap_or_default() {
			self.queue.pop_back();
		}
		let i = self.cur_time;
		self.queue.push_back((i, value));
		self.cur_time = self.cur_time.wrapping_add(1);
		while self.queue.front().map(|(i, _)| self.cur_time.wrapping_sub(*i) > self.size).unwrap_or_default() {
			self.queue.pop_front();
		}
	}

	fn get_min(&self) -> usize {
		self.queue.front().map(|(_, s)| *s).unwrap_or_default()
	}
}

impl AudioQueue {
	fn new(logger: &Logger, packet: InAudioBuf) -> Result<Self> {
		let data = packet.data().data();
		let last_packet_samples =
			packet::nb_samples(data.data(), SAMPLE_RATE)? * CHANNEL_NUM;
		let mut res = Self {
			decoder: Decoder::new(SAMPLE_RATE, CHANNELS)?,
			next_id: data.id(),
			whispering: false,
			packet_buffer: Default::default(),
			packet_buffer_samples: 0,
			decoded_buffer: Default::default(),
			decoded_pos: 0,
			last_packet_samples,
			packet_loss_num: 0,
			buffering_samples: 0,
			last_buffer_samples: SlidingWindowMinimum::new(LAST_BUFFER_SIZE_COUNT),
			buffered_for_samples: 0,
		};
		res.add_buffer_size(0);
		res.add_packet(logger, packet)?;
		Ok(res)
	}

	pub fn get_decoder(&self) -> &Decoder { &self.decoder }
	pub fn is_whispering(&self) -> bool { self.whispering }

	/// Size is in samples.
	fn add_buffer_size(&mut self, size: usize) {
		self.last_buffer_samples.push(size);
	}

	fn get_min_queue_size(&self) -> usize {
		self.last_packet_samples + self.last_buffer_samples.get_min()
	}

	fn add_packet(&mut self, logger: &Logger, packet: InAudioBuf) -> Result<()> {
		if self.packet_buffer.len() >= MAX_BUFFER_PACKETS {
			bail!("Audio queue is full, dropping");
		}
		let samples;
		if packet.data().data().data().is_empty() {
			// End of stream
			samples = 0;
		} else {
			samples = packet::nb_samples(packet.data().data().data(), SAMPLE_RATE)?;
		}

		let id = packet.data().data().id();
		let packet = QueuePacket {
			packet,
			samples,
			id,
		};
		if id.wrapping_sub(self.next_id) > MAX_BUFFER_PACKETS as u16 {
			bail!("Audio packet is too late, dropping");
		}
		// Put into first spot where the id is smaller
		let i = self.packet_buffer.len() - self.packet_buffer.iter().enumerate()
			.rev().take_while(|(_, p)| p.id.wrapping_sub(id) <= MAX_BUFFER_PACKETS as u16).count();
		trace!(logger, "Insert packet {} at {}", id, i);
		let last_id = self.packet_buffer.back().map(|p| p.id.wrapping_add(1)).unwrap_or(id);
		if last_id <= id {
			self.buffering_samples = self.buffering_samples.saturating_sub(samples);
			// Reduce buffering counter by lost packets if there are some
			self.buffering_samples = self.buffering_samples.saturating_sub(
				usize::from(id - last_id) * self.last_packet_samples);
		}

		self.packet_buffer_samples += packet.samples;
		self.packet_buffer.insert(i, packet);

		Ok(())
	}

	fn decode_packet(&mut self, logger: &Logger, packet: Option<&QueuePacket>, fec: bool) -> Result<()> {
		trace!(logger, "Decoding packet"; "has_packet" => packet.is_some(),
			"fec" => fec);
		let packet_data;
		let len;
		if let Some(p) = packet {
			packet_data = Some(p.packet.data().data().data());
			len = p.samples;
			self.whispering = matches!(p.packet.data().data(), AudioData::S2CWhisper { .. });
		} else {
			packet_data = None;
			len = self.last_packet_samples;
		}
		self.packet_loss_num += 1;

		self.decoded_buffer.resize(self.decoded_pos + len * CHANNEL_NUM, 0.0);
		let len = self.decoder.decode_float(
			packet_data,
			&mut self.decoded_buffer[self.decoded_pos..],
			fec,
		)?;
		self.last_packet_samples = len;
		self.decoded_buffer.truncate(self.decoded_pos + len * CHANNEL_NUM);

		// Update packet_loss_num
		if packet.is_some() && !fec {
			self.packet_loss_num = 0;
		}

		// Update last_buffer_samples
		let mut count = self.packet_buffer_samples;
		if let Some(last) = self.packet_buffer.back() {
			// Lost packets
			trace!(logger, "Ids"; "last_id" => last.id,
				"next_id" => self.next_id,
				"first_id" => self.packet_buffer.front().unwrap().id,
				"buffer_len" => self.packet_buffer.len(),
				"min_size" => self.get_min_queue_size());
			count += (usize::from(last.id.wrapping_sub(self.next_id))
				+ 1 - self.packet_buffer.len()) * self.last_packet_samples;
		}
		self.add_buffer_size(count);

		Ok(())
	}

	/// Decode data and return the requested length of buffered data.
	///
	/// Returns `true` in the second return value when the stream ended,
	/// `false` when it continues normally.
	pub fn get_next_data(&mut self, logger: &Logger, len: usize) -> Result<(&[f32], bool)> {
		if self.buffering_samples > 0 {
			if self.buffered_for_samples >= MAX_BUFFER_TIME {
				self.buffering_samples = 0;
				self.buffered_for_samples = 0;
				trace!(logger, "Buffered for too long";
					"buffered_for_samples" => self.buffered_for_samples,
					"buffering_samples" => self.buffering_samples);
			} else {
				self.buffered_for_samples += len;
				trace!(logger, "Buffering";
					"buffered_for_samples" => self.buffered_for_samples,
					"buffering_samples" => self.buffering_samples);
				return Ok((&[], false));
			}
		}

		while self.decoded_buffer.len() < self.decoded_pos + len {
			// Need to refill buffer
			if self.decoded_pos < self.decoded_buffer.len() {
				if self.decoded_pos > 0 {
					self.decoded_buffer.drain(..self.decoded_pos);
					self.decoded_pos = 0;
				}
			} else {
				self.decoded_buffer.clear();
				self.decoded_pos = 0;
			}

			// Decode a packet
			if let Some(packet) = self.packet_buffer.pop_front() {
				if packet.packet.data().data().data().is_empty() {
					return Ok((&self.decoded_buffer[self.decoded_pos..], true));
				}

				self.packet_buffer_samples -= packet.samples;
				let cur_id = self.next_id;
				self.next_id = self.next_id.wrapping_add(1);
				if packet.id > cur_id {
					// Packet loss
					debug!(logger, "Audio packet loss"; "need" => cur_id,
						"have" => packet.id);
					if packet.id == self.next_id {
						// Can use forward-error-correction
						self.decode_packet(logger, Some(&packet), true)?;
					} else {
						self.decode_packet(logger, None, false)?;
					}
					self.packet_buffer_samples += packet.samples;
					self.packet_buffer.push_front(packet);
				} else {
					debug_assert!(packet.id == cur_id, "Invalid packet queue state: {} != {}", packet.id, cur_id);
					self.decode_packet(logger, Some(&packet), false)?;
				}
			} else {
				// TODO This happens too often, we should buffer more if this happens
				debug!(logger, "No packets in queue");
				// Packet loss or end of stream
				self.decode_packet(logger, None, false)?;
			}

			// Check if we should speed-up playback
			let min = self.get_min_queue_size();
			let min_left = min - self.last_packet_samples;
			if min_left > MAX_BUFFER_SIZE {
				debug!(logger, "Truncating buffer"; "min_left" => min_left);
				// Throw out all but min samples
				let mut keep_samples = 0;
				let keep = self.packet_buffer.iter().rev().take_while(|p| {
					keep_samples += p.samples;
					keep_samples < min
				}).count();
				let len = self.packet_buffer.len() - keep;
				self.packet_buffer.drain(..len);
				self.packet_buffer_samples = self.packet_buffer.iter()
					.map(|p| p.samples).sum();
				if let Some(p) = self.packet_buffer.front() {
					self.next_id = p.id;
				}
			} else if min_left > self.last_packet_samples {
				// Speed-up
				debug!(logger, "Speed-up buffer"; "min" => min,
					"cur_packet_count" => self.packet_buffer.len(),
					"last_packet_samples" => self.last_packet_samples);
				let start = self.decoded_buffer.len() - self.last_packet_samples * CHANNEL_NUM;
				for i in 0..(self.last_packet_samples / SPEED_CHANGE_STEPS) {
					let i = start + i * (SPEED_CHANGE_STEPS - 1) * CHANNEL_NUM;
					self.decoded_buffer.drain(i..(i + CHANNEL_NUM));
				}
			}
		}

		let res = &self.decoded_buffer[self.decoded_pos..(self.decoded_pos + len)];
		self.decoded_pos += len;
		Ok((res, false))
	}
}

impl<Id: Clone + Eq + Hash + PartialEq> AudioHandler<Id> {
	pub fn new(logger: Logger) -> Self {
		Self {
			logger,
			queues: Default::default(),
			talkers_changed: false,
			avg_buffer_samples: 0,
		}
	}

	/// Delete all queues
	pub fn reset(&mut self) {
		self.queues.clear();
		self.talkers_changed = false;
	}

	pub fn get_queues(&self) -> &HashMap<Id, AudioQueue> { &self.queues }
	pub fn talkers_changed(&mut self) -> bool {
		if self.talkers_changed {
			self.talkers_changed = false;
			true
		} else {
			false
		}
	}

	/// `buf` is not cleared before filling it.
	pub fn fill_buffer(&mut self, buf: &mut [f32]) {
		trace!(self.logger, "Filling audio buffer"; "len" => buf.len());
		let mut to_remove = Vec::new();
		for (id, queue) in self.queues.iter_mut() {
			if queue.packet_loss_num >= MAX_PACKET_LOSSES {
				trace!(self.logger, "Removing talker";
					"packet_loss_num" => queue.packet_loss_num);
				to_remove.push(id.clone());
				continue;
			}

			match queue.get_next_data(&self.logger, buf.len()) {
				Err(e) => {
					warn!(self.logger, "Failed to decode audio packet";
						"error" => ?e);
				}
				Ok((r, is_end)) => {
					for i in 0..r.len() {
						buf[i] += r[i];
					}
					if is_end {
						to_remove.push(id.clone());
					}
				}
			}
		}

		for id in to_remove {
			self.queues.remove(&id);
			self.talkers_changed = true;
		}
	}

	pub fn handle_packet(&mut self, id: Id, packet: InAudioBuf) -> Result<()> {
		let empty = packet.data().data().data().is_empty();
		let codec = packet.data().data().codec();
		if codec != CodecType::OpusMusic && codec != CodecType::OpusVoice {
			bail!("Can only handle opus audio but got {:?}", codec);
		}

		if let Some(queue) = self.queues.get_mut(&id) {
			queue.add_packet(&self.logger, packet)?;
		} else {
			if empty {
				return Ok(());
			}

			trace!(self.logger, "Adding talker");
			let mut queue = AudioQueue::new(&self.logger, packet)?;
			if !self.queues.is_empty() {
				// Update avg_buffer_samples
				self.avg_buffer_samples = self.queues.values()
					.map(|q| q.get_min_queue_size()).sum::<usize>() / self.queues.len();
			}
			queue.buffering_samples = self.avg_buffer_samples;
			self.queues.insert(id, queue);
			self.talkers_changed = true;
		}
		Ok(())
	}
}

#[cfg(test)]
mod test {
	use std::sync::Mutex;

	use slog::{o, Drain};
	use tsproto_packets::packets::{Direction, OutAudio};

	use super::*;

	fn create_logger() -> Logger {
		let decorator =
			slog_term::PlainDecorator::new(slog_term::TestStdoutWriter);
		let drain = Mutex::new(slog_term::FullFormat::new(decorator).build())
				.fuse();

		slog::Logger::root(drain, o!())
	}

	#[test]
	fn sliding_window_minimum() {
		let data = &[(5, 5), (6, 5), (3, 3), (4, 3), (6, 3), (5, 3), (6, 3),
			(6, 4), (6, 5), (6, 5), (6, 6), (7, 6), (5, 5)];
		let mut window = SlidingWindowMinimum::new(5);
		assert_eq!(window.get_min(), 0);
		for (i, (val, min)) in data.iter().enumerate() {
			println!("{:?}", window);
			window.push(*val);
			assert_eq!(window.get_min(), *min, "Failed in iteration {} ({:?})",
				i, window);
		}
	}

	#[test]
	fn packets_wrapping() {
		let logger = create_logger();
		let mut handler = AudioHandler::<ClientId>::new(logger);
		let id = ClientId(0);
		let mut buf = vec![0.0; 48_000 / 100 * 2];

		for i in 0..100 {
			let packet = OutAudio::new(&AudioData::S2C {
				id: 65_500u16.wrapping_add(i),
				codec: CodecType::OpusMusic,
				from: 0,
				data: &[0, 0, 0, 0, 0, 0, 0],
			});
			let input = InAudioBuf::try_new(Direction::S2C, packet.into_vec()).unwrap();
			handler.handle_packet(id, input).unwrap();

			if i > 5 {
				handler.fill_buffer(&mut buf);
			}
		}
	}
}
