extern crate base64;
extern crate openssl;
extern crate structopt;
extern crate tsproto;

use std::io::Write;

use structopt::clap::AppSettings;
use structopt::StructOpt;
use tsproto::algorithms as algs;
use tsproto::utils;

#[derive(StructOpt, Debug)]
#[structopt(raw(
	global_settings = "&[AppSettings::ColoredHelp, \
	                   AppSettings::VersionlessSubcommands]"
))]
struct Args {
	#[structopt(short = "d", long = "data", help = "Data (hex)")]
	data: String,
	#[structopt(short = "c", long = "client", help = "Is client")]
	is_client: bool,
}

fn main() {
	tsproto::init().unwrap();

	// Parse command line options
	let args = Args::from_args();

	let data = utils::read_hex(&args.data).unwrap();
	let (header, mut data) = utils::parse_packet(data, args.is_client).unwrap();
	algs::decrypt_fake(&header, &mut data).unwrap();
	std::io::stdout().write_all(&data).unwrap();
}
