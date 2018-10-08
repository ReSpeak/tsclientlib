extern crate base64;
extern crate curve25519_dalek;
extern crate openssl;
extern crate structopt;
extern crate tsproto;

use curve25519_dalek::edwards::CompressedEdwardsY;
use curve25519_dalek::scalar::Scalar;
use structopt::clap::AppSettings;
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
#[structopt(raw(
	global_settings = "&[AppSettings::ColoredHelp, \
	                   AppSettings::VersionlessSubcommands]"
))]
struct Args {
	#[structopt(short = "e", long = "pub", help = "Public key")]
	pub_key: String,
	#[structopt(short = "d", long = "priv", help = "Private key")]
	priv_key: String,
}

fn main() {
	tsproto::init().unwrap();

	// Parse command line options
	let args = Args::from_args();

	let pub_key = base64::decode(&args.pub_key).unwrap();
	let priv_key = base64::decode(&args.priv_key).unwrap();
	let mut pubk = [0; 32];
	pubk.copy_from_slice(&pub_key);
	let mut privk = [0; 32];
	privk.copy_from_slice(&priv_key);
	/*privk[0] &= 248;
    privk[31] &= 63;
    privk[31] |= 64;*/

	let priv_scal = Scalar::from_bytes_mod_order(privk);
	//let priv_scal = Scalar::from_bytes_mod_order(privk);
	let pub_point_compr = CompressedEdwardsY(pubk);
	let pub_point = -pub_point_compr.decompress().unwrap();
	//let pub_point = curve25519_dalek::constants::ED25519_BASEPOINT_POINT;

	let res = (pub_point * priv_scal).compress();
	println!("Result: {}", base64::encode(&res.0));
}
