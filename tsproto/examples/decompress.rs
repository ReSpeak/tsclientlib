extern crate quicklz;
extern crate structopt;
extern crate tsproto;

use std::io::{Cursor, Write};

use structopt::clap::AppSettings;
use structopt::StructOpt;
use tsproto::utils;

#[derive(StructOpt, Debug)]
#[structopt(raw(global_settings = "&[AppSettings::ColoredHelp, \
                                   AppSettings::VersionlessSubcommands]"))]
struct Args {
	#[structopt(help = "Data (hex)")]
	data: String,
}

fn main() {
	tsproto::init().unwrap();

	// Parse command line options
	let args = Args::from_args();

	let data = utils::read_hex(&args.data).unwrap();
	let data =
		quicklz::decompress(&mut Cursor::new(data), std::u32::MAX).unwrap();
	std::io::stdout().write_all(&data).unwrap();
}
