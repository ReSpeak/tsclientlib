extern crate tsproto_util;

use std::env;
use std::fs::File;
use std::io::prelude::*;
use std::path::Path;

use tsproto_util::{Declaration, Packets};

fn main() {
	let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
	let base_dir = format!("{}/..", manifest_dir);
	let out_dir = env::var("OUT_DIR").unwrap();

	// Read packets
	let decls = Packets::from_file(&base_dir, ());

	// Write packets
	let path = Path::new(&out_dir);
	let mut structs = File::create(&path.join("packets.rs")).unwrap();
	write!(&mut structs, "{}", decls).unwrap();
}
