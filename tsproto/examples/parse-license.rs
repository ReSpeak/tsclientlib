use structopt::StructOpt;
use tsproto::license::*;

#[derive(StructOpt, Debug)]
#[structopt(author, about)]
struct Args {
	/// The license data (base64)
	#[structopt()]
	license: String,
}

fn main() {
	// Parse command line options
	let args = Args::from_args();

	let l = Licenses::parse(base64::decode(&args.license).unwrap()).unwrap();
	println!("{:#?}", l);
}
