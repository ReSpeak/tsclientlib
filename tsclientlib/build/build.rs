use std::env;
use std::fs::File;
use std::io::prelude::*;
use std::path::Path;

mod book_to_messages_parser;
mod facade_parser;

use crate::book_to_messages_parser::BookToMessagesDeclarations;
use crate::facade_parser::FacadeDeclarations;

fn main() {
	let out_dir = env::var("OUT_DIR").unwrap();
	let path = Path::new(&out_dir);

	// Book to messages
	let mut structs = File::create(&path.join("b2mdecls.rs")).unwrap();
	write!(&mut structs, "{}", BookToMessagesDeclarations::default()).unwrap();

	// Facades
	let mut structs = File::create(&path.join("facades.rs")).unwrap();
	write!(&mut structs, "{}", FacadeDeclarations::default()).unwrap();
}
