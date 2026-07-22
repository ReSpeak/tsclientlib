use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::{Result, format_err};
use audiopus::coder::Encoder;
use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::{Device, Stream, StreamConfig};
use futures::prelude::*;
use tokio::sync::mpsc;
use tokio::task::LocalSet;
use tokio::time::{self, Duration};
use tokio_stream::wrappers::IntervalStream;
use tracing::{debug, error, instrument, warn};
use tsproto_packets::packets::{AudioData, CodecType, OutAudio, OutPacket};

use super::*;

pub struct AudioToTs {
	device: Device,
	listener: Arc<Mutex<Option<mpsc::Sender<OutPacket>>>>,
	stream: Option<Stream>,

	is_playing: bool,
	/// Storing an f32
	volume: Arc<AtomicU32>,
}

struct Callback {
	listener: Arc<Mutex<Option<mpsc::Sender<OutPacket>>>>,
	encoder: Encoder,
	/// Storing an f32
	volume: Arc<AtomicU32>,

	tmp_buffer: [f32; USUAL_FRAME_SIZE * 2],
	opus_output: [u8; MAX_OPUS_FRAME_SIZE],
}

impl AudioToTs {
	pub fn new(device: Device, local_set: &LocalSet) -> Result<Arc<Mutex<Self>>> {
		let listener = Arc::new(Mutex::new(Default::default()));
		let volume = Arc::new(AtomicU32::new(1.0_f32.to_bits()));

		let res = Arc::new(Mutex::new(Self {
			device,
			listener,
			stream: None,

			is_playing: false,
			volume,
		}));

		Self::start(res.clone(), local_set);

		Ok(res)
	}

	#[instrument(skip(self))]
	fn open_capture(&self) -> Result<Stream> {
		let config = StreamConfig {
			channels: 1,
			sample_rate: 48000,
			buffer_size: cpal::BufferSize::Fixed(USUAL_FRAME_SIZE as u32),
		};

		let encoder = Encoder::new(
			audiopus::SampleRate::Hz48000,
			audiopus::Channels::Mono,
			audiopus::Application::Voip,
		)
		.expect("Could not create encoder");
		let mut callback = Callback {
			listener: self.listener.clone(),
			encoder,
			volume: self.volume.clone(),

			tmp_buffer: [0.0; _],
			opus_output: [0; _],
		};

		self.device
			.build_input_stream(
				config,
				move |audio_data, _| callback.callback(audio_data),
				|error| error!(%error, "Error during audio capture"),
				Some(Duration::from_secs(5)),
			)
			.map_err(|e| format_err!("cpal error: {}", e))
	}

	pub fn set_listener(&self, sender: mpsc::Sender<OutPacket>) {
		let mut listener = self.listener.lock().unwrap();
		*listener = Some(sender);
	}

	pub fn set_volume(&mut self, volume: f32) {
		self.volume.store(volume.to_bits(), Ordering::Relaxed);
	}

	pub fn set_playing(&mut self, playing: bool) {
		if let Some(stream) = &self.stream {
			if playing {
				if let Err(error) = stream.play() {
					error!(%error, "Failed to start stream");
					self.stream = None;
				} else {
					self.is_playing = true;
				}
			} else {
				if let Err(error) = stream.pause() {
					error!(%error, "Failed to pause stream");
					self.stream = None;
				}
				self.is_playing = false;
			}
		}
	}

	#[instrument(skip(a2t, local_set))]
	fn start(a2t: Arc<Mutex<Self>>, local_set: &LocalSet) {
		local_set.spawn_local(
			IntervalStream::new(time::interval(Duration::from_secs(1))).for_each(move |_| {
				let mut a2t = a2t.lock().unwrap();

				if a2t.stream.is_none() {
					// Try to reconnect to audio
					match a2t.open_capture() {
						Ok(s) => {
							a2t.stream = Some(s);
							debug!("Reconnected to capture device");
							if a2t.is_playing {
								a2t.set_playing(true);
							}
						}
						Err(error) => {
							error!(%error, "Failed to open capture device");
						}
					};
				}

				future::ready(())
			}),
		);
	}
}

impl Callback {
	#[instrument(skip(self, buffer))]
	fn callback<'a>(&'a mut self, mut buffer: &'a [f32]) {
		// Handle volume
		let volume = f32::from_bits(self.volume.load(Ordering::Relaxed));
		if volume != 1.0 {
			if self.tmp_buffer.len() < buffer.len() {
				warn!("tmp buffer len smaller than received capture buffer, dropping data");
			}
			for (dst, src) in self.tmp_buffer.iter_mut().zip(buffer.iter()) {
				*dst = *src * volume;
			}
			buffer = &self.tmp_buffer[..buffer.len()];
		}

		match self.encoder.encode_float(buffer, &mut self.opus_output[..]) {
			Err(error) => {
				error!(%error, "Failed to encode opus");
			}
			Ok(len) => {
				// Create packet
				let packet = OutAudio::new(&AudioData::C2S {
					id: 0,
					codec: CodecType::OpusVoice,
					data: &self.opus_output[..len],
				});

				// Write into packet sink
				let mut listener = self.listener.lock().unwrap();
				if let Some(lis) = &mut *listener {
					match lis.try_send(packet) {
						Err(mpsc::error::TrySendError::Closed(_)) => *listener = None,
						_ => {}
					}
				}
			}
		}
	}
}
