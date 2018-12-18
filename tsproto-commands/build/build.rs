extern crate itertools;
#[macro_use]
extern crate t4rust_derive;
extern crate tsproto_structs;
extern crate tsproto_util;

use std::env;
use std::fs::File;
use std::io::prelude::*;
use std::path::Path;

use tsproto_util::{Errors, Versions};

mod message_parser;
use message_parser::MessageDeclarations;

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
