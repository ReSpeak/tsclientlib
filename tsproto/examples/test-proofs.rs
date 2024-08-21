use base64::prelude::*;
use clap::Parser;
use tsproto_types::crypto::EccKeyPubP256;

#[derive(Parser, Debug)]
#[command(author, about)]
struct Args {
	/// Public key
	#[arg(short, long)]
	key: String,
	/// Data (base64)
	#[arg(short, long)]
	data: String,
	/// Signature (base64)
	#[arg(short, long)]
	signature: String,
}

fn main() {
	// Parse command line options
	let args = Args::parse();

	// l → proof
	// ek || beta → proof

	let data = BASE64_STANDARD.decode(&args.data).unwrap();
	let signature = BASE64_STANDARD.decode(&args.signature).unwrap();
	let key = EccKeyPubP256::from_ts(&args.key).unwrap();
	/*let keyts = tomcrypt::P256EccKey::import(&BASE64_STANDARD.decode(&args.key).unwrap())
		.unwrap();
	let res = keyts.verify_hash(&data, &signature).unwrap();
	println!("Res: {:?}", res);*/

	key.verify(&data, &signature).unwrap();
}
