use std::env;
use std::fs::File;
use std::io::prelude::*;
use std::path::Path;

mod error_parser;
mod version_parser;

use crate::error_parser::Errors;
use crate::version_parser::Versions;

fn main() {
	let out_dir = env::var("OUT_DIR").unwrap();

	// Error declarations
	let path = Path::new(&out_dir);
	let mut structs = File::create(&path.join("errors.rs")).unwrap();
	write!(&mut structs, "{}", Errors::default()).unwrap();

	// Write versions
	let mut structs = File::create(&path.join("versions.rs")).unwrap();
	write!(&mut structs, "{}", Versions::default()).unwrap();
}
