use std::env;
use std::fs::File;
use std::io::prelude::*;
use std::path::Path;

mod facade_parser;

use crate::facade_parser::FacadeDeclarations;

fn main() {
	built::write_built_file()
		.expect("Failed to acquire build-time information");

	let out_dir = env::var("OUT_DIR").unwrap();
	let path = Path::new(&out_dir);

	// Facades
	let mut structs = File::create(&path.join("facades.rs")).unwrap();
	write!(&mut structs, "{}", FacadeDeclarations::default()).unwrap();
}
