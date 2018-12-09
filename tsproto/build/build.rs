extern crate tsproto_util;

use std::env;
use std::fs::File;
use std::io::prelude::*;
use std::path::Path;

use tsproto_util::PacketDeclarations;

fn main() {
	let out_dir = env::var("OUT_DIR").unwrap();

	// Write packets
	let path = Path::new(&out_dir);
	let mut structs = File::create(&path.join("packets.rs")).unwrap();
	write!(&mut structs, "{}", PacketDeclarations).unwrap();
}
