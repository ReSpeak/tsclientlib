use curve25519_dalek::edwards::CompressedEdwardsY;
use curve25519_dalek::scalar::Scalar;
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
#[structopt(author, about)]
struct Args {
	/// Public key
	#[structopt(short, long = "pub")]
	pub_key: String,
	/// Private key
	#[structopt(short, long = "priv")]
	priv_key: String,
}

fn main() {
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
