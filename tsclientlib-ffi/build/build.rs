extern crate tsproto_util;

use std::env;
use std::fs::File;
use std::io::prelude::*;
use std::path::Path;

use tsproto_util::{BookDeclarations, BookFfi, Declaration};

fn main() {
	let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
	let base_dir = format!("{}/..", manifest_dir);
	let out_dir = env::var("OUT_DIR").unwrap();

	// Read declarations
	let decls = BookDeclarations::from_file(&base_dir, ());

	// Write declarations
	let path = Path::new(&out_dir);
	let mut structs = File::create(&path.join("book_ffi.rs")).unwrap();
	let book_ffi = BookFfi(&decls);
	write!(&mut structs, "{}", book_ffi).unwrap();
}
