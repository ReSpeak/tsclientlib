use std::sync::{Arc, Mutex};

use anyhow::Result;
use cpal::traits::HostTrait;
use tokio::task::LocalSet;

use audio_to_ts::AudioToTs;
use ts_to_audio::TsToAudio;

pub mod audio_to_ts;
pub mod ts_to_audio;

/// The usual frame size.
///
/// Use 48 kHz, 20 ms frames (50 per second) and mono data (1 channel).
/// This means 1920 samples and 7.5 kiB.
const USUAL_FRAME_SIZE: usize = 48000 / 50;

/// The maximum size of an opus frame is 1275 as from RFC6716.
const MAX_OPUS_FRAME_SIZE: usize = 1275;

#[derive(Clone)]
pub struct AudioData {
	pub a2ts: Arc<Mutex<AudioToTs>>,
	pub ts2a: Arc<Mutex<TsToAudio>>,
}

pub(crate) fn start(local_set: &LocalSet) -> Result<AudioData> {
	let host = cpal::default_host();
	let output_device = host.default_output_device().unwrap();
	let input_device = host.default_input_device().unwrap();

	let ts2a = TsToAudio::new(output_device, local_set)?;
	let a2ts = AudioToTs::new(input_device, local_set)?;

	Ok(AudioData { a2ts, ts2a })
}
