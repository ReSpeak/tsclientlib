#[macro_use]
extern crate t4rust_derive;

use std::env;
use std::fs::File;
use std::io::prelude::*;
use std::path::Path;

mod book_parser;
mod facade_parser;
mod messages_to_book_parser;

use crate::book_parser::BookDeclarations;
use crate::facade_parser::FacadeDeclarations;
use crate::messages_to_book_parser::MessagesToBookDeclarations;

fn main() {
	let out_dir = env::var("OUT_DIR").unwrap();

	// Book declarations
	let path = Path::new(&out_dir);
	let mut structs = File::create(&path.join("structs.rs")).unwrap();
	write!(&mut structs, "{}", BookDeclarations::default()).unwrap();

	// Messages to book
	let mut structs = File::create(&path.join("m2bdecls.rs")).unwrap();
	write!(&mut structs, "{}", MessagesToBookDeclarations::default()).unwrap();

	// Facades
	let mut structs = File::create(&path.join("facades.rs")).unwrap();
	write!(&mut structs, "{}", FacadeDeclarations::default()).unwrap();
}
