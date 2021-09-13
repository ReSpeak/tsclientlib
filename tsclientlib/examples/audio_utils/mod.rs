use std::sync::{Arc, Mutex};

use anyhow::Result;
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
	let sdl_context = sdl2::init().unwrap();

	let audio_subsystem = sdl_context.audio().unwrap();
	// SDL automatically disables the screensaver, enable it again
	if let Ok(video_subsystem) = sdl_context.video() {
		video_subsystem.enable_screen_saver();
	}

	let ts2a = TsToAudio::new(audio_subsystem.clone(), local_set)?;
	let a2ts = AudioToTs::new(audio_subsystem, local_set)?;

	Ok(AudioData { a2ts, ts2a })
}
