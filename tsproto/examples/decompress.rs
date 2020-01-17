use std::io::{Cursor, Write};

use structopt::StructOpt;
use tsproto::utils;

#[derive(StructOpt, Debug)]
#[structopt(author, about)]
struct Args {
	/// Data (hex)
	#[structopt()]
	data: String,
}

fn main() {
	// Parse command line options
	let args = Args::from_args();

	let data = utils::read_hex(&args.data).unwrap();
	let data =
		quicklz::decompress(&mut Cursor::new(data), std::u32::MAX).unwrap();
	std::io::stdout().write_all(&data).unwrap();
}
