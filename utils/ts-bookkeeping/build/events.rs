use std::collections::HashSet;
use std::default::Default;
use std::ops::Deref;

use heck::*;
use t4rust_derive::Template;
use tsproto_structs::book::*;
use tsproto_structs::convert_type;
use tsproto_structs::messages_to_book::{self, MessagesToBookDeclarations};

#[derive(Template)]
#[TemplatePath = "build/Events.tt"]
#[derive(Debug)]
pub struct EventDeclarations<'a>(
	&'a BookDeclarations,
	&'a MessagesToBookDeclarations<'a>,
);

impl<'a> Deref for EventDeclarations<'a> {
	type Target = BookDeclarations;
	fn deref(&self) -> &Self::Target { &self.0 }
}

impl Default for EventDeclarations<'static> {
	fn default() -> Self { EventDeclarations(&DATA, &messages_to_book::DATA) }
}

pub fn get_rust_type(p: &Property) -> String {
	let res = convert_type(&p.type_s, false);
	if p.opt { format!("Option<{}>", res) } else { res }
}

pub fn get_rust_ref_type(p: &Property) -> String {
	let res = convert_type(&p.type_s, true);
	if p.opt { format!("Option<{}>", res) } else { res }
}

fn get_ids(structs: &[Struct], struc: &Struct) -> String {
	let mut res = String::new();
	for id in &struc.id {
		let p = id.find_property(structs);
		if !res.is_empty() {
			res.push_str(", ");
		}
		res.push_str(&p.get_rust_type());
	}
	res
}

pub fn get_property_name(p: &Property) -> &str {
	if p.modifier.is_some() && p.name.ends_with('s') {
		&p.name[..p.name.len() - 1]
	} else {
		&p.name
	}
}

pub fn get_properties<'a>(
	structs: &'a [Struct],
	s: &'a Struct,
) -> Vec<&'a Property>
{
	s.properties
		.iter()
		.filter(|p| !structs.iter().any(|s| s.name == p.type_s))
		.collect()
}
