use std::collections::HashSet;
use std::default::Default;
use std::ops::Deref;

use t4rust_derive::Template;
use tsproto_structs::convert_type;
use tsproto_structs::book::*;
use tsproto_structs::messages_to_book::{self, MessagesToBookDeclarations,
	RuleKind};
use tsproto_util::{to_pascal_case, to_snake_case};

use crate::*;

#[derive(Template)]
#[TemplatePath = "build/Events.tt"]
#[derive(Debug)]
pub struct Events<'a>(
	&'a BookDeclarations,
	&'a MessagesToBookDeclarations<'a>,
);

impl<'a> Deref for Events<'a> {
	type Target = BookDeclarations;
	fn deref(&self) -> &Self::Target { &self.0 }
}

impl Default for Events<'static> {
	fn default() -> Self { Events(&DATA, &messages_to_book::DATA) }
}

pub fn get_rust_type(p: &Property) -> String {
	let res = convert_type(&p.type_s, false);
	if p.opt {
		format!("Option<{}>", res)
	} else {
		res
	}
}

fn get_ids(structs: &[Struct], struc: &Struct, p: &Property) -> Vec<String> {
	let mut ids = get_struct_ids(structs, struc);
	if let Some(m) = &p.modifier {
		if m == "map" {
			// The key is part of the id
			ids.push(p.key.as_ref().unwrap().to_string());
		} else if m == "array" {
			// Take the element itself as port of the id.
			// It has to be copied but most of the times it is an id itself.
			ids.push(get_ffi_type(&p.type_s));
		} else {
			panic!("Unknown modifier {}", m);
		}
	}
	ids
}

fn get_struct_ids(structs: &[Struct], struc: &Struct) -> Vec<String> {
	struc.id.iter().map(|i| get_ffi_type(&i.find_property(structs).type_s))
		.collect()
}

pub fn get_property_name(p: &Property) -> &str {
	if p.modifier.is_some() && p.name.ends_with('s') {
		&p.name[..p.name.len() - 1]
	} else {
		&p.name
	}
}

/// Add only things which are in messages to book, which actually will be
/// changed.
pub fn get_event_properties<'a>(structs: &'a [Struct],
	m2b: &'a MessagesToBookDeclarations<'a>, s: &'a Struct)
	-> Vec<&'a Property> {
	// All properties which are set at some point
	let set_props = m2b.decls.iter().filter(|e| e.book_struct.name == s.name)
		.flat_map(|e| e.rules.iter()).flat_map(|r| -> Box<Iterator<Item=_>> {
			match r {
				RuleKind::Map { to, .. } => Box::new(std::iter::once(to)),
				RuleKind::Function { to, .. } => Box::new(to.iter()),
			}
		}).map(|p| &p.name).collect::<HashSet<_>>();

	s.properties.iter().filter(|p| {
		if structs.iter().any(|s| s.name == p.type_s) {
			return false;
		}
		set_props.contains(&p.name)
	}).collect()
}
