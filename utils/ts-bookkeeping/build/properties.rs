//! Access properties of a connection with the property structs from events.
use std::default::Default;
use std::fmt::Write;

use heck::*;
use t4rust_derive::Template;
use tsproto_structs::book::*;
use tsproto_structs::embrace;
use tsproto_structs::messages_to_book::{self, MessagesToBookDeclarations};

#[derive(Template)]
#[TemplatePath = "build/Properties.tt"]
#[derive(Debug)]
pub struct Properties<'a>(&'a BookDeclarations, &'a MessagesToBookDeclarations<'a>);

impl Default for Properties<'static> {
	fn default() -> Self { Properties(&DATA, &messages_to_book::DATA) }
}

fn get_ids(struc: &Struct) -> String {
	let mut res = String::new();
	for i in 0..struc.id.len() {
		if !res.is_empty() {
			res.push_str(", ");
		}
		let _ = write!(res, "s{}", i);
	}
	res
}

fn get_ids2(structs: &[Struct], struc: &Struct) -> String {
	let mut res = String::new();
	for (i, id) in struc.id.iter().enumerate() {
		let p = id.find_property(structs);
		if !res.is_empty() {
			res.push_str(", ");
		}
		if p.type_s != "str" {
			res.push('*');
		}
		let _ = write!(res, "s{}", i);
	}
	res
}
