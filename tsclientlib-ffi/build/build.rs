#[macro_use]
extern crate t4rust_derive;

use std::env;
use std::fs::File;
use std::io::prelude::*;
use std::path::Path;

mod book_ffi;
use crate::book_ffi::BookFfi;

fn main() {
	let out_dir = env::var("OUT_DIR").unwrap();

	// Write declarations
	let path = Path::new(&out_dir);
	let mut structs = File::create(&path.join("book_ffi.rs")).unwrap();
	write!(&mut structs, "{}", BookFfi::default()).unwrap();
}
