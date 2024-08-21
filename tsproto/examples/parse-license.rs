use base64::prelude::*;
use clap::Parser;
use tsproto::license::*;

#[derive(Parser, Debug)]
#[command(author, about)]
struct Args {
	/// The license data (base64)
	#[arg()]
	license: String,
}

fn main() {
	// Parse command line options
	let args = Args::parse();

	let l = Licenses::parse(BASE64_STANDARD.decode(&args.license).unwrap()).unwrap();
	println!("{:#?}", l);
}
