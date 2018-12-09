extern crate base64;
extern crate structopt;
extern crate tsproto;

use structopt::clap::AppSettings;
use structopt::StructOpt;
use tsproto::license::*;

#[derive(StructOpt, Debug)]
#[structopt(raw(global_settings = "&[AppSettings::ColoredHelp, \
                                   AppSettings::VersionlessSubcommands]"))]
struct Args {
	#[structopt(help = "The license data (base64)")]
	license: String,
}

fn main() {
	tsproto::init().unwrap();

	// Parse command line options
	let args = Args::from_args();

	let l = Licenses::parse(&base64::decode(&args.license).unwrap()).unwrap();
	println!("{:?}", l);
}
