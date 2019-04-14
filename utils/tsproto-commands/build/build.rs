use std::env;
use std::fs::File;
use std::io::prelude::*;
use std::path::Path;

mod error_parser;
mod message_parser;
mod version_parser;

use crate::error_parser::Errors;
use crate::message_parser::MessageDeclarations;
use crate::version_parser::Versions;

fn main() {
	let out_dir = env::var("OUT_DIR").unwrap();

	// Error declarations
	let path = Path::new(&out_dir);
	let mut structs = File::create(&path.join("errors.rs")).unwrap();
	write!(&mut structs, "{}", Errors::default()).unwrap();

	// Write messages
	let decls = MessageDeclarations::s2c();
	let mut structs = File::create(&path.join("s2c_messages.rs")).unwrap();
	write!(&mut structs, "{}", MessageDeclarations(&decls)).unwrap();

	let decls = MessageDeclarations::c2s();
	let mut structs = File::create(&path.join("c2s_messages.rs")).unwrap();
	write!(&mut structs, "{}", MessageDeclarations(&decls)).unwrap();

	// Write versions
	let mut structs = File::create(&path.join("versions.rs")).unwrap();
	write!(&mut structs, "{}", Versions::default()).unwrap();
}
