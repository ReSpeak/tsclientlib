extern crate base64;
extern crate openssl;
extern crate structopt;
extern crate tsproto;

use std::io::Write;

use structopt::clap::AppSettings;
use structopt::StructOpt;
use tsproto::algorithms as algs;
use tsproto::packets::*;
use tsproto::utils;

#[derive(StructOpt, Debug)]
#[structopt(raw(global_settings = "&[AppSettings::ColoredHelp, \
                                   AppSettings::VersionlessSubcommands]"))]
struct Args {
	#[structopt(short = "d", long = "debug", help = "Print backtrace")]
	debug: bool,
	#[structopt(short = "c", long = "client", help = "Server to client")]
	c2s: bool,
	#[structopt(help = "Data (hex)")]
	data: String,
}

fn main() {
	tsproto::init().unwrap();

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
