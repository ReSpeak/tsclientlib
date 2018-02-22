extern crate base64;
extern crate openssl;
extern crate structopt;
#[macro_use]
extern crate structopt_derive;
extern crate tsproto;
#[cfg(feature = "tomcrypt")]
extern crate tomcrypt;

#[cfg(feature = "tomcrypt")]
use structopt::StructOpt;
use structopt::clap::AppSettings;
//use tsproto::algorithms as algs;
#[cfg(feature = "tomcrypt")]
use tsproto::crypto::EccKey;

#[derive(StructOpt, Debug)]
#[structopt(raw(global_settings =
    "&[AppSettings::ColoredHelp, AppSettings::VersionlessSubcommands]"))]
struct Args {
    #[structopt(short = "k", long = "key", help = "Public key")]
    key: String,
    #[structopt(short = "d", long = "data", help = "Data (base64)")]
    data: String,
    #[structopt(short = "s", long = "signature", help = "Signature (base64)")]
    signature: String,
}

#[cfg(feature = "tomcrypt")]
fn main() {
    tsproto::init().unwrap();

    // Parse command line options
    let args = Args::from_args();

    // l → proof
    // ek || beta → proof

    let data = base64::decode(&args.data).unwrap();
    let signature = base64::decode(&args.signature).unwrap();
    let key = EccKey::from_ts(&args.key).unwrap();
    /*let keyts = tomcrypt::EccKey::import(&base64::decode(&args.key).unwrap())
        .unwrap();
    let res = keyts.verify_hash(&data, &signature).unwrap();
    println!("Res: {:?}", res);*/

    key.verify(&data, &signature).unwrap();
}

#[cfg(not(feature = "tomcrypt"))]
fn main() {}
