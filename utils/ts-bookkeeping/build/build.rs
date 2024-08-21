use std::env;
use std::fs::File;
use std::io::prelude::*;
use std::path::Path;

mod book_parser;
mod book_to_messages_parser;
mod events;
mod message_parser;
mod messages_to_book_parser;
mod properties;

use crate::book_parser::BookDeclarations;
use crate::book_to_messages_parser::BookToMessagesDeclarations;
use crate::events::EventDeclarations;
use crate::message_parser::MessageDeclarations;
use crate::messages_to_book_parser::MessagesToBookDeclarations;
use crate::properties::Properties;

fn main() {
	let out_dir = env::var("OUT_DIR").unwrap();

	// Book declarations
	let path = Path::new(&out_dir);
	let mut structs = File::create(path.join("structs.rs")).unwrap();
	write!(&mut structs, "{}", BookDeclarations::default()).unwrap();

	// Messages to book
	let mut structs = File::create(path.join("m2bdecls.rs")).unwrap();
	write!(&mut structs, "{}", MessagesToBookDeclarations::default()).unwrap();

	// Write messages
	let decls = MessageDeclarations::s2c();
	let mut structs = File::create(path.join("s2c_messages.rs")).unwrap();
	write!(&mut structs, "{}", MessageDeclarations(&decls)).unwrap();

	let decls = MessageDeclarations::c2s();
	let mut structs = File::create(path.join("c2s_messages.rs")).unwrap();
	write!(&mut structs, "{}", MessageDeclarations(&decls)).unwrap();

	// Book to messages
	let mut structs = File::create(path.join("b2mdecls.rs")).unwrap();
	write!(&mut structs, "{}", BookToMessagesDeclarations::default()).unwrap();

	// Events
	let mut structs = File::create(path.join("events.rs")).unwrap();
	write!(&mut structs, "{}", EventDeclarations::default()).unwrap();

	// Properties
	let mut structs = File::create(path.join("properties.rs")).unwrap();
	write!(&mut structs, "{}", Properties::default()).unwrap();
}
