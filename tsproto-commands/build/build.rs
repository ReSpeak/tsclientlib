extern crate tsproto_util;

use std::env;
use std::fs::File;
use std::io::prelude::*;
use std::path::Path;

use tsproto_util::{Errors, MessageDeclarations, Versions};

fn main() {
	let out_dir = env::var("OUT_DIR").unwrap();

	// Error declarations
	let path = Path::new(&out_dir);
	let mut structs = File::create(&path.join("errors.rs")).unwrap();
	write!(&mut structs, "{}", Errors::default()).unwrap();

	// Write messages
	let mut structs = File::create(&path.join("messages.rs")).unwrap();
	write!(&mut structs, "{}", MessageDeclarations::default()).unwrap();

	// Write versions
	let mut structs = File::create(&path.join("versions.rs")).unwrap();
	write!(&mut structs, "{}", Versions::default()).unwrap();
}
