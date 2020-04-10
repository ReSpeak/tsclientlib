use structopt::StructOpt;
use tsproto_types::crypto::EccKeyPubP256;

#[derive(StructOpt, Debug)]
#[structopt(author, about)]
struct Args {
	/// Public key
	#[structopt(short = "k", long)]
	key: String,
	/// Data (base64)
	#[structopt(short = "d", long)]
	data: String,
	/// Signature (base64)
	#[structopt(short = "s", long)]
	signature: String,
}

fn main() {
	// Parse command line options
	let args = Args::from_args();

	// l → proof
	// ek || beta → proof

	let data = base64::decode(&args.data).unwrap();
	let signature = base64::decode(&args.signature).unwrap();
	let key = EccKeyPubP256::from_ts(&args.key).unwrap();
	/*let keyts = tomcrypt::P256EccKey::import(&base64::decode(&args.key).unwrap())
		.unwrap();
	let res = keyts.verify_hash(&data, &signature).unwrap();
	println!("Res: {:?}", res);*/

	key.verify(&data, &signature).unwrap();
}
