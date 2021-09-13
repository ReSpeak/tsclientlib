use std::sync::{Arc, Mutex};

use anyhow::{format_err, Result};
use futures::prelude::*;
use sdl2::audio::{AudioCallback, AudioDevice, AudioSpecDesired, AudioStatus};
use sdl2::AudioSubsystem;
use tokio::task::LocalSet;
use tokio::time::{self, Duration};
use tokio_stream::wrappers::IntervalStream;
use tracing::{debug, error, instrument};
use tsclientlib::ClientId;
use tsproto_packets::packets::InAudioBuf;

use super::*;
use crate::ConnectionId;

type Id = (ConnectionId, ClientId);
type AudioHandler = tsclientlib::audio::AudioHandler<Id>;

pub struct TsToAudio {
	audio_subsystem: AudioSubsystem,
	device: AudioDevice<SdlCallback>,
	data: Arc<Mutex<AudioHandler>>,
}

struct SdlCallback {
	data: Arc<Mutex<AudioHandler>>,
}

impl TsToAudio {
	pub fn new(audio_subsystem: AudioSubsystem, local_set: &LocalSet) -> Result<Arc<Mutex<Self>>> {
		let data = Arc::new(Mutex::new(AudioHandler::new()));

		let device = Self::open_playback(&audio_subsystem, data.clone())?;

		let res = Arc::new(Mutex::new(Self { audio_subsystem, device, data }));

		Self::start(res.clone(), local_set);

		Ok(res)
	}

	#[instrument(skip(audio_subsystem, data))]
	fn open_playback(
		audio_subsystem: &AudioSubsystem, data: Arc<Mutex<AudioHandler>>,
	) -> Result<AudioDevice<SdlCallback>> {
		let desired_spec = AudioSpecDesired {
			freq: Some(48000),
			channels: Some(2),
			samples: Some(USUAL_FRAME_SIZE as u16),
		};

		audio_subsystem
			.open_playback(None, &desired_spec, move |spec| {
				// This spec will always be the desired spec, the sdl wrapper passes
				// zero as `allowed_changes`.
				debug!(?spec, driver = audio_subsystem.current_audio_driver(), "Got playback spec");
				SdlCallback { data }
			})
			.map_err(|e| format_err!("SDL error: {}", e))
	}

	#[instrument(skip(t2a, local_set))]
	fn start(t2a: Arc<Mutex<Self>>, local_set: &LocalSet) {
		local_set.spawn_local(
			IntervalStream::new(time::interval(Duration::from_secs(1))).for_each(move |_| {
				let mut t2a = t2a.lock().unwrap();

				if t2a.device.status() == AudioStatus::Stopped {
					// Try to reconnect to audio
					match Self::open_playback(&t2a.audio_subsystem, t2a.data.clone()) {
						Ok(d) => {
							t2a.device = d;
							debug!("Reconnected to playback device");
						}
						Err(error) => {
							error!(%error, "Failed to open playback device");
						}
					};
				}

				let data_empty = t2a.data.lock().unwrap().get_queues().is_empty();
				if t2a.device.status() == AudioStatus::Paused && !data_empty {
					debug!("Resuming playback");
					t2a.device.resume();
				} else if t2a.device.status() == AudioStatus::Playing && data_empty {
					debug!("Pausing playback");
					t2a.device.pause();
				}
				future::ready(())
			}),
		);
	}

	#[instrument(skip(self, id, packet))]
	pub(crate) fn play_packet(&mut self, id: Id, packet: InAudioBuf) -> Result<()> {
		let mut data = self.data.lock().unwrap();
		data.handle_packet(id, packet)?;

		if self.device.status() == AudioStatus::Paused {
			debug!("Resuming playback");
			self.device.resume();
		}
		Ok(())
	}
}

impl AudioCallback for SdlCallback {
	type Channel = f32;
	fn callback(&mut self, buffer: &mut [Self::Channel]) {
		// Clear buffer
		for d in &mut *buffer {
			*d = 0.0;
		}

		let mut data = self.data.lock().unwrap();
		data.fill_buffer(buffer);
	}
}
