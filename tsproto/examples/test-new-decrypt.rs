extern crate base64;
extern crate openssl;
extern crate ring;
extern crate structopt;
#[macro_use]
extern crate structopt_derive;
extern crate tsproto;

use std::io::{Cursor, Write};

use structopt::StructOpt;
use structopt::clap::AppSettings;
use tsproto::algorithms as algs;
use tsproto::utils;

use ring::digest;

#[derive(StructOpt, Debug)]
#[structopt(raw(global_settings =
    "&[AppSettings::ColoredHelp, AppSettings::VersionlessSubcommands]"))]
struct Args {
    /*#[structopt(short = "d", long = "data", help = "Data (hex)")]
    data: String,
    #[structopt(short = "c", long = "client", help = "Is client")]
    is_client: bool,*/
}

fn main() {
    tsproto::init().unwrap();

    // Parse command line options
    let args = Args::from_args();

    /*let data = utils::read_hex(&args.data).unwrap();
    let (header, mut data) = utils::parse_packet(data, args.is_client).unwrap();
    algs::decrypt_fake(&header, &mut data).unwrap();
    std::io::stdout().write_all(&data).unwrap();*/

    let shared_iv = utils::read_hex("C2 45 6F CB FC 22 08 AE 44 2B 7D E7 3A 67 1B DA 93 09 B2 00 F2 CD 10 49 08 CD 3A B0 7B DD 58 AD").unwrap();
    let mut shared_iv = digest::digest(&digest::SHA512, &shared_iv).as_ref().to_vec();
    assert_eq!(utils::read_hex("4D 3F DA B7 D8 B0 2C 82 70 6A 39 3E 97 17 61 09 FA 03 AB 30 5C BB 78 7A 9A 82 D5 39 9A 60 FD A9 F6 7A D9 04 52 F2 AE 00 3E 35 E8 19 10 89 40 43 80 58 27 1F 0A E0 E0 3D E0 9C F0 A3 4D 15 6B F0").unwrap().as_slice(), shared_iv.as_slice());

    let alpha = [0; 10];
    let beta = base64::decode("I4onb0zMyAD6bd24QANDls40eOES7qmjonBFtt5wRWzAfIIQWTSxjEas6TGTZIJ8QSJNX+Pl").unwrap();

    for i in 0..10 {
        shared_iv[i] ^= alpha[i];
    }
    for i in 0..54 {
        shared_iv[i + 10] ^= beta[i];
    }

    let mut temp = [0; 70];
    temp[0] = 0x31;
    temp[1] = 0x2 & 0xf;
    temp[6..].copy_from_slice(&shared_iv);

    let keynonce = digest::digest(&digest::SHA256, &temp);
    let keynonce = keynonce.as_ref();

    let mut all = utils::read_hex("2b982443ab38be6b00020000329abf64d4572e1349897b5e1e96fbc4a763a4c4ce1f64f0c1e3febd0a5f04a82ab1f2bc2344bb374fd16181beb8233b5b06944280470e9b6893290a1da0776ffcd89f3beec2ce23b9694930c09efaaea0d88a6895a08ede4d5cbfea61291fc553ac651f1e2bc1d2bd277a8bd9ab5386415579a9e56fac46d8b6b119f454bebd99179cd317dec60af205341d11f274d02bbacdd7e9773f72a426358ca1d39016dd95bde2409cd81bf99b340887e997ea982370c6790cf4d23150460820224766838ea4ec4d71dd102ede701ea0001f392623aa410dd9ab0e45874da82e29e6e370515ec30a37dd73f5a364c233ff014384beab5f1708c9f48dfba33a520f8fcdcef055789c54693c3fe72c5bfaca7cb4ca1fed77b8624660b8abc882f4b95b1284cb6dc55019c6082dd6dd146fa50383662d7298bef04ababaf1af80e15cd4c1f81326f085788e2918e00324147dce39b23db71326abc3de4b94df10f1531e9cce202bba71fa3ebeefd77b21fa3260a62e92eeee2183421d384a8c48777e2f9efbc58d4f442c5f0529c7c0e27e81b2b6b1b05eb8fa19256886248d553582dfd24c7cfab3c3f7317a5cebc6504b53fa0e86fc8c1100fc1d506fcf96caa76a7c0b6a27e577f2efdecd4070e847a559bf37d75bfdbe9e814c702426ce696d8645bc300b5f28f9e7f1ce").unwrap();

    let header = tsproto::packets::Header::read(&false, &mut Cursor::new(&all)).unwrap();
    let data = &mut all[..];
    let mut key = [0; 16];
    let mut nonce = [0; 16];
    key.copy_from_slice(&keynonce[..16]);
    nonce.copy_from_slice(&keynonce[16..]);

    let expected_key = utils::read_hex("D2 42 75 71 9C EE 83 35 EF 8A CE E0 B7 28 40 B8").unwrap();
    let expected_nonce = utils::read_hex("9C D9 30 B7 58 FE 50 23 64 66 11 C5 36 0E A2 5F").unwrap();

    assert_eq!(&expected_nonce, &nonce);
    assert_eq!(&expected_key, &key);

    algs::decrypt_key_nonce(&header, data, &key, &nonce).unwrap();

    println!("Decrypted: {:?}", String::from_utf8_lossy(data));
}
