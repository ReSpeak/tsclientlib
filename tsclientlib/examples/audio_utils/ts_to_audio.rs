use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::fmt;
use std::sync::Arc;
use std::time::{Duration, Instant};

use audiopus::coder::{Decoder, GenericCtl};
use failure::{format_err, Error};
use futures::prelude::*;
use parking_lot::Mutex;
use sdl2::audio::{AudioCallback, AudioDevice, AudioSpecDesired, AudioStatus};
use sdl2::AudioSubsystem;
use slog::{debug, error, o, trace, Logger};
use tokio::timer::Interval;
use tsclientlib::ClientId;
use tsproto_packets::packets::{AudioData, CodecType, InAudio};

use super::*;
use crate::ConnectionId;

/// After this amount of seconds, a decoder will be removed.
const VOICE_TIMEOUT_SECS: u64 = 1;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct Id {
	con: ConnectionId,
	client: ClientId,
}

pub struct TsToAudio {
	logger: Logger,
	audio_subsystem: AudioSubsystem,
	device: AudioDevice<SdlCallback>,
	/// For each client, store the opus decoder and the instant when it was last
	/// used.
	decoders: HashMap<Id, (Decoder, Instant)>,
	/// The audio queue, new data is appended at the end and data is loaded
	/// from the beginning.
	data: Arc<Mutex<HashMap<Id, VecDeque<f32>>>>,

	/// Decoded opus data
	opus_output: Vec<f32>,
}

struct SdlCallback {
	logger: Logger,
	data: Arc<Mutex<HashMap<Id, VecDeque<f32>>>>,
}

impl fmt::Display for Id {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "{}-{}", self.con.0, self.client.0)
	}
}

impl TsToAudio {
	pub fn new(
		logger: Logger,
		audio_subsystem: AudioSubsystem,
	) -> Result<Arc<Mutex<Self>>, Error>
	{
		let logger = logger.new(o!("pipeline" => "ts-to-audio"));
		let data = Arc::new(Mutex::new(Default::default()));

		let device = Self::open_playback(
			logger.clone(),
			&audio_subsystem,
			data.clone(),
		)?;

		let res = Arc::new(Mutex::new(Self {
			logger,
			audio_subsystem,
			device,
			decoders: Default::default(),
			data,

			opus_output: vec![0f32; USUAL_FRAME_SIZE],
		}));

		Self::start(res.clone());

		Ok(res)
	}

	fn open_playback(
		logger: Logger,
		audio_subsystem: &AudioSubsystem,
		data: Arc<Mutex<HashMap<Id, VecDeque<f32>>>>,
	) -> Result<AudioDevice<SdlCallback>, Error>
	{
		let desired_spec = AudioSpecDesired {
			freq: Some(48000),
			channels: Some(2),
			samples: Some(USUAL_SAMPLE_COUNT as u16),
		};

		audio_subsystem.open_playback(None, &desired_spec, move |spec| {
			// This spec will always be the desired spec, the sdl wrapper passes
			// zero as `allowed_changes`.
			debug!(logger, "Got playback spec"; "spec" => ?spec, "driver" => audio_subsystem.current_audio_driver());
			SdlCallback {
				logger,
				data,
			}
		}).map_err(|e| format_err!("SDL error: {}", e))
	}

	fn start(t2a: Arc<Mutex<Self>>) {
		let logger = t2a.lock().logger.clone();
		tokio::runtime::current_thread::spawn(
			Interval::new_interval(Duration::from_secs(1))
				.for_each(move |_| {
					let mut t2a = t2a.lock();
					if !t2a.decoders.is_empty() {
						// Check for inactive connections
						let now = Instant::now();
						let dur = Duration::from_secs(VOICE_TIMEOUT_SECS);
						let logger = t2a.logger.clone();
						t2a.decoders.retain(|id, (_, last)| {
							if now.duration_since(*last) > dur {
								trace!(logger, "Removing stale connection"; "id" => %id);
								false
							} else {
								true
							}
						});

						if t2a.decoders.is_empty() {
							debug!(logger, "Pausing playback");
							t2a.device.pause();
						}
					}

					if t2a.device.status() == AudioStatus::Stopped {
						// Try to reconnect to audio
						match Self::open_playback(
							t2a.logger.clone(),
							&t2a.audio_subsystem,
							t2a.data.clone(),
						) {
							Ok(d) => {
								t2a.device = d;
								debug!(
									t2a.logger,
									"Reconnected to playback device"
								);
								if !t2a.decoders.is_empty() {
									t2a.device.resume();
								}
							}
							Err(e) => {
								error!(t2a.logger, "Failed to open playback device"; "error" => ?e);
							}
						};
					}
					Ok(())
				})
				.map_err(
					move |e| error!(logger, "t2a interval failed"; "error" => ?e),
				),
		);
	}

	pub(crate) fn play_packet(
		&mut self,
		con: ConnectionId,
		packet: &InAudio,
	) -> Result<(), Error>
	{
		if let AudioData::S2C { id: _, from, codec, data }
		| AudioData::S2CWhisper { id: _, from, codec, data } = packet.data()
		{
			if *codec != CodecType::OpusVoice && *codec != CodecType::OpusMusic
			{
				return Err(format_err!(
					"Got unsupported audio codec, only opus is supported"
				));
			}

			let id = Id { con, client: ClientId(*from) };
			let channels = self.device.spec().channels;
			let was_empty = self.decoders.is_empty();

			let mut tmp_entry;
			let decoder = match self.decoders.entry(id) {
				Entry::Occupied(o) => {
					tmp_entry = o;
					let entry = tmp_entry.get_mut();
					entry.1 = Instant::now();
					&mut entry.0
				}
				Entry::Vacant(v) => {
					debug!(self.logger, "Creating opus decoder"; "id" => %id);
					let opus_channels = if channels == 1 {
						audiopus::Channels::Mono
					} else {
						audiopus::Channels::Stereo
					};

					// Always use the channel count of SDL, opus automatically
					// averages or duplicates samples for each channel.
					let decoder = Decoder::new(
						audiopus::SampleRate::Hz48000,
						opus_channels,
					)?;
					&mut v.insert((decoder, Instant::now())).0
				}
			};

			if data.is_empty() {
				debug!(self.logger, "Resetting decoder"; "id" => %id);
				decoder.reset_state()?;
				return Ok(());
			}

			let len = loop {
				match decoder.decode_float(
					*data,
					&mut self.opus_output[..],
					false,
				) {
					Ok(len) => break len,
					Err(audiopus::error::Error::Opus(
						audiopus::error::ErrorCode::BufferTooSmall,
					)) => {
						// Enlarge the buffer
						if self.opus_output.len() == MAX_FRAME_SIZE {
							return Err(format_err!(
								"Bad opus packet, maximum buffer size exceeded"
							));
						} else if self.opus_output.len() * 2 > MAX_FRAME_SIZE {
							self.opus_output.resize(MAX_FRAME_SIZE, 0f32);
						} else {
							self.opus_output
								.resize(self.opus_output.len() * 2, 0f32);
						}
					}
					Err(e) => return Err(e.into()),
				}
			};

			// Shrink the buffer
			let size = len * usize::from(channels);
			if size <= self.opus_output.len() / 2 {
				self.opus_output.truncate(len);
			}
			trace!(self.logger, "Decoded opus packet"; "id" => %id, "len" => len);

			// Put into queue
			{
				let mut data = self.data.lock();
				let queue = data.entry(id).or_insert_with(Default::default);
				if queue.len() > size * 2 {
					debug!(self.logger, "Removing samples from playback queue"; "id" => %id, "count" => queue.len() - size);
					*queue = queue.split_off(queue.len() - size);
					queue.clear();
				}
				queue.extend(self.opus_output[..size].iter());
			}

			if was_empty {
				self.device.resume();
			}
		}
		Ok(())
	}
}

impl AudioCallback for SdlCallback {
	type Channel = f32;
	fn callback(&mut self, buffer: &mut [Self::Channel]) {
		trace!(self.logger, "Filling audio playback buffer"; "len" => buffer.len());
		// Fill the buffer with silence
		for d in &mut *buffer {
			*d = 0.0;
		}

		// Mix data
		let mut data = self.data.lock();
		data.retain(|id, queue| {

			let len = std::cmp::min(buffer.len(), queue.len());
			let (a, b) = queue.as_slices();
			let alen = std::cmp::min(a.len(), len);
			for i in 0..alen {
				buffer[i] += a[i];
			}
			if alen < len {
				for i in 0..len - alen {
					buffer[alen + i] += b[i];
				}
			}

			if queue.len() == len {
				trace!(self.logger, "Remove playback queue buffer"; "id" => %id);
				false
			} else {
				*queue = queue.split_off(len);
				trace!(self.logger, "Left playback queue buffer"; "id" => %id, "len" => queue.len());
				true
			}
		});
	}
}
