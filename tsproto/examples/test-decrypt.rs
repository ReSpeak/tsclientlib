use std::io::Write;

use clap::Parser;
use tsproto::algorithms as algs;
use tsproto::utils;
use tsproto_packets::packets::*;

#[derive(Parser, Debug)]
#[command(author, about)]
struct Args {
	/// Print backtrace
	#[arg(short, long = "debug")]
	debug: bool,
	/// Server to client
	#[arg(short, long = "client")]
	c2s: bool,
	/// Data (hex)
	#[arg()]
	data: String,
}

fn main() {
	// Parse command line options
	let args = Args::parse();

	let dir = if args.c2s { Direction::C2S } else { Direction::S2C };

	let data = utils::read_hex(&args.data).unwrap();
	let packet = match InPacket::try_new(dir, &data) {
		Ok(p) => p,
		Err(e) => {
			if args.debug {
				println!("Failed to decode: {:?}", e);
			} else {
				println!("Failed to decode: {}", e);
			}
			return;
		}
	};
	let decrypted = match algs::decrypt_fake(&packet) {
		Ok(d) => d,
		Err(e) => {
			if args.debug {
				println!("Failed to decrypt: {:?}", e);
			} else {
				println!("Failed to decrypt: {}", e);
			}
			return;
		}
	};
	std::io::stdout().write_all(&decrypted).unwrap();
}
