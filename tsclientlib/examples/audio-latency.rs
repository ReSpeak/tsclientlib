use anyhow::Result;
use clap::Parser;
use tokio::sync::mpsc;
use tokio::task::LocalSet;
use tracing::{debug, info};

use tsclientlib::ClientId;
use tsproto_packets::packets::{Direction, InAudioBuf};

mod audio_utils;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct ConnectionId(u64);

#[derive(Parser, Debug)]
#[command(author, about)]
struct Args {
	/// The volume for the capturing
	#[arg(default_value_t = 1.0)]
	volume: f32,
}

#[tokio::main]
async fn main() -> Result<()> { real_main().await }

async fn real_main() -> Result<()> {
	tracing_subscriber::fmt::init();

	// Parse command line options
	let args = Args::parse();

	let con_id = ConnectionId(0);
	let local_set = LocalSet::new();
	let audiodata = audio_utils::start(&local_set)?;

	let (send, mut recv) = mpsc::channel(1);
	{
		let mut a2t = audiodata.a2ts.lock().unwrap();
		a2t.set_listener(send);
		a2t.set_volume(args.volume);
		a2t.set_playing(true);
	}

	let t2a = audiodata.ts2a.clone();
	loop {
		// Wait for ctrl + c
		tokio::select! {
			send_audio = recv.recv() => {
				if let Some(packet) = send_audio {
					let from = ClientId(0);
					let mut t2a = t2a.lock().unwrap();
					let in_audio = InAudioBuf::try_new(Direction::C2S, packet.into_vec()).unwrap();
					if let Err(error) = t2a.play_packet((con_id, from), in_audio) {
						debug!(%error, "Failed to play packet");
					}
				} else {
					info!("Audio sending stream was canceled");
					break;
				}
			}
			_ = tokio::signal::ctrl_c() => { break; }
		};
	}

	Ok(())
}
