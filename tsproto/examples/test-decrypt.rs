use std::io::Write;

use structopt::StructOpt;
use tsproto::algorithms as algs;
use tsproto::utils;
use tsproto_packets::packets::*;

#[derive(StructOpt, Debug)]
#[structopt(author, about)]
struct Args {
	/// Print backtrace
	#[structopt(short, long = "debug")]
	debug: bool,
	/// Server to client
	#[structopt(short, long = "client")]
	c2s: bool,
	/// Data (hex)
	#[structopt()]
	data: String,
}

fn main() {
	// Parse command line options
	let args = Args::from_args();

	let dir = if args.c2s { Direction::C2S } else { Direction::S2C };

	let data = utils::read_hex(&args.data).unwrap();
	let packet = match InPacket::try_new(data.into(), dir) {
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
