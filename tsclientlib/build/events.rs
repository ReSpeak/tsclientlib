use std::collections::HashSet;
use std::default::Default;
use std::ops::Deref;

use tsproto_structs::convert_type;
use tsproto_structs::book::*;
use tsproto_structs::messages_to_book::{self, MessagesToBookDeclarations,
	RuleKind};

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

fn get_rust_type(p: &Property) -> String {
	let res = convert_type(&p.type_s, false);
	if p.opt {
		format!("Option<{}>", res)
	} else {
		res
	}
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
