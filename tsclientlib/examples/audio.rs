//! This example connects to a teamspeak instance and joins a channel.
//! The connected bot-identity will listen on your microphone and send everything to the current
//! channel. In parallel, the program will play back all other clients in the channel via its own
//! audio output.
//! Microphone -> Channel
//! Channel -> PC Speakers

use anyhow::{bail, Result};
use clap::Parser;
use futures::prelude::*;
use tokio::sync::mpsc;
use tokio::task::LocalSet;
use tracing::{debug, info};

use tsclientlib::{ClientId, Connection, DisconnectOptions, Identity, StreamItem};
use tsproto_packets::packets::AudioData;

mod audio_utils;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct ConnectionId(u64);

#[derive(Parser, Debug)]
#[command(author, about)]
struct Args {
	/// The address of the server to connect to
	#[arg(short, long, default_value = "localhost")]
	address: String,
	/// The volume for the capturing
	#[arg(default_value_t = 1.0)]
	volume: f32,
	/// Print the content of all packets
	///
	/// 0. Print nothing
	/// 1. Print command string
	/// 2. Print packets
	/// 3. Print udp packets
	#[arg(short, long, action = clap::ArgAction::Count)]
	verbose: u8,
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

	let con_config = Connection::build(args.address)
		.log_commands(args.verbose >= 1)
		.log_packets(args.verbose >= 2)
		.log_udp_packets(args.verbose >= 3);

	// Optionally set the key of this client, otherwise a new key is generated.
	let id = Identity::new_from_str(
		"MG0DAgeAAgEgAiAIXJBlj1hQbaH0Eq0DuLlCmH8bl+veTAO2+\
		k9EQjEYSgIgNnImcmKo7ls5mExb6skfK2Tw+u54aeDr0OP1ITs\
		C/50CIA8M5nmDBnmDM/gZ//4AAAAAAAAAAAAAAAAAAAAZRzOI").unwrap();
	let con_config = con_config.identity(id);

	// Connect
	let mut con = con_config.connect()?;

	let r = con
		.events()
		.try_filter(|e| future::ready(matches!(e, StreamItem::BookEvents(_))))
		.next()
		.await;
	if let Some(r) = r {
		r?;
	}

	let (send, mut recv) = mpsc::channel(5);
	{
		let mut a2t = audiodata.a2ts.lock().unwrap();
		a2t.set_listener(send);
		a2t.set_volume(args.volume);
		a2t.set_playing(true);
	}

	loop {
		let t2a = audiodata.ts2a.clone();
		let events = con.events().try_for_each(|e| async {
			if let StreamItem::Audio(packet) = e {
				let from = ClientId(match packet.data().data() {
					AudioData::S2C { from, .. } => *from,
					AudioData::S2CWhisper { from, .. } => *from,
					_ => panic!("Can only handle S2C packets but got a C2S packet"),
				});
				let mut t2a = t2a.lock().unwrap();
				if let Err(error) = t2a.play_packet((con_id, from), packet) {
					debug!(%error, "Failed to play packet");
				}
			}
			Ok(())
		});

		// Wait for ctrl + c
		tokio::select! {
			send_audio = recv.recv() => {
				if let Some(packet) = send_audio {
					con.send_audio(packet)?;
				} else {
					info!("Audio sending stream was canceled");
					break;
				}
			}
			_ = tokio::signal::ctrl_c() => { break; }
			r = events => {
				r?;
				bail!("Disconnected");
			}
		};
	}

	// Disconnect
	con.disconnect(DisconnectOptions::new())?;
	con.events().for_each(|_| future::ready(())).await;

	Ok(())
}
