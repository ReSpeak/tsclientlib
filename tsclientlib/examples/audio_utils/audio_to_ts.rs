use std::sync::{Arc, Mutex};

use anyhow::{format_err, Result};
use audiopus::coder::Encoder;
use futures::prelude::*;
use sdl2::audio::{AudioCallback, AudioDevice, AudioSpec, AudioSpecDesired, AudioStatus};
use sdl2::AudioSubsystem;
use tokio::sync::mpsc;
use tokio::task::LocalSet;
use tokio::time::{self, Duration};
use tokio_stream::wrappers::IntervalStream;
use tracing::{debug, error, instrument};
use tsproto_packets::packets::{AudioData, CodecType, OutAudio, OutPacket};

use super::*;

pub struct AudioToTs {
	audio_subsystem: AudioSubsystem,
	listener: Arc<Mutex<Option<mpsc::Sender<OutPacket>>>>,
	device: AudioDevice<SdlCallback>,

	is_playing: bool,
	volume: Arc<Mutex<f32>>,
}

struct SdlCallback {
	spec: AudioSpec,
	encoder: Encoder,
	listener: Arc<Mutex<Option<mpsc::Sender<OutPacket>>>>,
	volume: Arc<Mutex<f32>>,

	opus_output: [u8; MAX_OPUS_FRAME_SIZE],
}

impl AudioToTs {
	pub fn new(audio_subsystem: AudioSubsystem, local_set: &LocalSet) -> Result<Arc<Mutex<Self>>> {
		let listener = Arc::new(Mutex::new(Default::default()));
		let volume = Arc::new(Mutex::new(1.0));

		let device = Self::open_capture(&audio_subsystem, listener.clone(), volume.clone())?;

		let res = Arc::new(Mutex::new(Self {
			audio_subsystem,
			listener,
			device,

			is_playing: false,
			volume,
		}));

		Self::start(res.clone(), local_set);

		Ok(res)
	}

	#[instrument(skip(audio_subsystem, listener, volume))]
	fn open_capture(
		audio_subsystem: &AudioSubsystem, listener: Arc<Mutex<Option<mpsc::Sender<OutPacket>>>>,
		volume: Arc<Mutex<f32>>,
	) -> Result<AudioDevice<SdlCallback>> {
		let desired_spec = AudioSpecDesired {
			freq: Some(48000),
			channels: Some(1),
			// Default sample size, 20 ms per packet
			samples: Some(48000 / 50),
		};

		audio_subsystem
			.open_capture(None, &desired_spec, |spec| {
				// This spec will always be the desired spec, the sdl wrapper passes
				// zero as `allowed_changes`.
				debug!(?spec, driver = audio_subsystem.current_audio_driver(), "Got capture spec");
				let opus_channels = if spec.channels == 1 {
					audiopus::Channels::Mono
				} else {
					audiopus::Channels::Stereo
				};

				let encoder = Encoder::new(
					audiopus::SampleRate::Hz48000,
					opus_channels,
					audiopus::Application::Voip,
				)
				.expect("Could not create encoder");

				SdlCallback {
					spec,
					encoder,
					listener,
					volume,

					opus_output: [0; MAX_OPUS_FRAME_SIZE],
				}
			})
			.map_err(|e| format_err!("SDL error: {}", e))
	}

	pub fn set_listener(&self, sender: mpsc::Sender<OutPacket>) {
		let mut listener = self.listener.lock().unwrap();
		*listener = Some(sender);
	}

	pub fn set_volume(&mut self, volume: f32) { *self.volume.lock().unwrap() = volume; }

	pub fn set_playing(&mut self, playing: bool) {
		if playing {
			self.device.resume();
		} else {
			self.device.pause();
		}
		self.is_playing = playing;
	}

	#[instrument(skip(a2t, local_set))]
	fn start(a2t: Arc<Mutex<Self>>, local_set: &LocalSet) {
		local_set.spawn_local(
			IntervalStream::new(time::interval(Duration::from_secs(1))).for_each(move |_| {
				let mut a2t = a2t.lock().unwrap();
				if a2t.device.status() == AudioStatus::Stopped {
					// Try to reconnect to audio
					match Self::open_capture(
						&a2t.audio_subsystem,
						a2t.listener.clone(),
						a2t.volume.clone(),
					) {
						Ok(d) => {
							a2t.device = d;
							debug!("Reconnected to capture device");
							if a2t.is_playing {
								a2t.device.resume();
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

impl AudioCallback for SdlCallback {
	type Channel = f32;

	#[instrument(skip(self, buffer))]
	fn callback(&mut self, buffer: &mut [Self::Channel]) {
		// Handle volume
		let volume = *self.volume.lock().unwrap();
		if volume != 1.0 {
			for d in &mut *buffer {
				*d *= volume;
			}
		}

		match self.encoder.encode_float(buffer, &mut self.opus_output[..]) {
			Err(error) => {
				error!(%error, "Failed to encode opus");
			}
			Ok(len) => {
				// Create packet
				let codec = if self.spec.channels == 1 {
					CodecType::OpusVoice
				} else {
					CodecType::OpusMusic
				};
				let packet =
					OutAudio::new(&AudioData::C2S { id: 0, codec, data: &self.opus_output[..len] });

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
