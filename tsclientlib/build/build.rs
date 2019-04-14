use std::env;
use std::fs::File;
use std::io::prelude::*;
use std::path::Path;

mod book_parser;
mod book_to_messages_parser;
mod events;
mod facade_parser;
mod messages_to_book_parser;
mod properties;

use crate::book_parser::BookDeclarations;
use crate::book_to_messages_parser::BookToMessagesDeclarations;
use crate::events::EventDeclarations;
use crate::facade_parser::FacadeDeclarations;
use crate::messages_to_book_parser::MessagesToBookDeclarations;
use crate::properties::Properties;

fn main() {
	built::write_built_file()
		.expect("Failed to acquire build-time information");

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

	// Book to messages
	let mut structs = File::create(&path.join("b2mdecls.rs")).unwrap();
	write!(&mut structs, "{}", BookToMessagesDeclarations::default()).unwrap();

	// Events
	let mut structs = File::create(&path.join("events.rs")).unwrap();
	write!(&mut structs, "{}", EventDeclarations::default()).unwrap();

	// Properties
	let mut structs = File::create(&path.join("properties.rs")).unwrap();
	write!(&mut structs, "{}", Properties::default()).unwrap();
}
