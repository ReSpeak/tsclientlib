use std::sync::{Arc, Mutex};

use anyhow::{Result, format_err};
use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::{Device, Stream, StreamConfig};
use futures::prelude::*;
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
	device: Device,
	stream: Option<Stream>,
	is_playing: bool,
	data: Arc<Mutex<AudioHandler>>,
}

struct Callback {
	data: Arc<Mutex<AudioHandler>>,
}

impl TsToAudio {
	pub fn new(device: Device, local_set: &LocalSet) -> Result<Arc<Mutex<Self>>> {
		let data = Arc::new(Mutex::new(AudioHandler::new()));

		let res = Arc::new(Mutex::new(Self { device, stream: None, is_playing: false, data }));

		Self::start(res.clone(), local_set);

		Ok(res)
	}

	#[instrument(skip(self))]
	fn open_playback(&self) -> Result<Stream> {
		let config = StreamConfig {
			channels: 2,
			sample_rate: 48000,
			buffer_size: cpal::BufferSize::Fixed(USUAL_FRAME_SIZE as u32),
		};

		let mut callback = Callback { data: self.data.clone() };

		self.device
			.build_output_stream(
				config,
				move |audio_data, _| callback.callback(audio_data),
				|error| error!(%error, "Error during audio playback"),
				Some(Duration::from_secs(5)),
			)
			.map_err(|e| format_err!("cpal error: {}", e))
	}

	#[instrument(skip(t2a, local_set))]
	fn start(t2a: Arc<Mutex<Self>>, local_set: &LocalSet) {
		local_set.spawn_local(
			IntervalStream::new(time::interval(Duration::from_secs(1))).for_each(move |_| {
				let mut t2a = t2a.lock().unwrap();

				if t2a.stream.is_none() {
					// Try to reconnect to audio
					match t2a.open_playback() {
						Ok(s) => {
							t2a.stream = Some(s);
							t2a.is_playing = false;
							debug!("Reconnected to playback device");
						}
						Err(error) => {
							error!(%error, "Failed to open playback device");
						}
					};
				}

				if let Some(stream) = &t2a.stream {
					let data_empty = t2a.data.lock().unwrap().get_queues().is_empty();
					if !t2a.is_playing && !data_empty {
						debug!("Resuming playback");
						if let Err(error) = stream.play() {
							error!(%error, "Failed to start stream");
							t2a.stream = None;
						} else {
							t2a.is_playing = true;
						}
					} else if t2a.is_playing && data_empty {
						debug!("Pausing playback");
						if let Err(error) = stream.pause() {
							error!(%error, "Failed to pause stream");
							t2a.stream = None;
						}
						t2a.is_playing = false;
					}
				}
				future::ready(())
			}),
		);
	}

	#[instrument(skip(self, id, packet))]
	pub(crate) fn play_packet(&mut self, id: Id, packet: InAudioBuf) -> Result<()> {
		{
			let mut data = self.data.lock().unwrap();
			data.handle_packet(id, packet)?;
		}

		if !self.is_playing {
			if let Some(stream) = &self.stream {
				debug!("Resuming playback");
				if let Err(error) = stream.play() {
					error!(%error, "Failed to start stream");
					self.stream = None;
				} else {
					self.is_playing = true;
				}
			}
		}
		Ok(())
	}
}

impl Callback {
	fn callback(&mut self, buffer: &mut [f32]) {
		// Clear buffer
		for d in &mut *buffer {
			*d = 0.0;
		}

		let mut data = self.data.lock().unwrap();
		data.fill_buffer(buffer);
	}
}
