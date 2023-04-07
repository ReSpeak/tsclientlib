use std::io::{Cursor, Write};

use clap::Parser;
use tsproto::utils;

#[derive(Parser, Debug)]
#[command(author, about)]
struct Args {
	/// Data (hex)
	#[arg()]
	data: String,
}

fn main() {
	// Parse command line options
	let args = Args::parse();

	let data = utils::read_hex(&args.data).unwrap();
	let data = quicklz::decompress(&mut Cursor::new(data), std::u32::MAX).unwrap();
	std::io::stdout().write_all(&data).unwrap();
}
