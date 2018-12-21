use std::env;
use std::fs::File;
use std::io::prelude::*;
use std::path::Path;

use tsproto_util::BookFfi;

fn main() {
	let out_dir = env::var("OUT_DIR").unwrap();

	// Write declarations
	let path = Path::new(&out_dir);
	let mut structs = File::create(&path.join("book_ffi.rs")).unwrap();
	let book_ffi = BookFfi(&tsproto_structs::book::DATA);
	write!(&mut structs, "{}", book_ffi).unwrap();
}
