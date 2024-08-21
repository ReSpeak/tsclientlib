use std::env;
use std::fs::File;
use std::io::prelude::*;
use std::path::Path;

mod enums;
mod errors;
mod versions;

use crate::enums::Enums;
use crate::errors::Errors;
use crate::versions::Versions;

fn main() {
	let out_dir = env::var("OUT_DIR").unwrap();

	// Enums
	let path = Path::new(&out_dir);
	let mut structs = File::create(path.join("enums.rs")).unwrap();
	write!(&mut structs, "{}", Enums).unwrap();

	// Errors
	let path = Path::new(&out_dir);
	let mut structs = File::create(path.join("errors.rs")).unwrap();
	write!(&mut structs, "{}", Errors).unwrap();

	// Versions
	let mut structs = File::create(path.join("versions.rs")).unwrap();
	write!(&mut structs, "{}", Versions).unwrap();
}
