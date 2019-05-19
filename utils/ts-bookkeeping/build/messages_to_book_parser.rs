use std::default::Default;
use std::ops::Deref;

use heck::*;
use t4rust_derive::Template;
use tsproto_structs::book::{PropId, Property};
use tsproto_structs::messages::Field;
use tsproto_structs::messages_to_book::*;
use tsproto_structs::*;

use crate::events::get_rust_type;

#[derive(Template)]
#[TemplatePath = "build/MessagesToBook.tt"]
#[derive(Debug)]
pub struct MessagesToBookDeclarations<'a>(
	&'a messages_to_book::MessagesToBookDeclarations<'a>,
);

impl<'a> Deref for MessagesToBookDeclarations<'a> {
	type Target = messages_to_book::MessagesToBookDeclarations<'a>;
	fn deref(&self) -> &Self::Target { &self.0 }
}

impl Default for MessagesToBookDeclarations<'static> {
	fn default() -> Self { MessagesToBookDeclarations(&DATA) }
}

fn get_id_args(event: &Event) -> String {
	let mut res = String::new();
	for f in &event.id {
		if !res.is_empty() {
			res.push_str(", ");
		}
		if is_ref_type(&f.get_rust_type("", false)) {
			res.push('&');
		}
		res.push_str(&format!("{{
			let val = cmd.get_arg(\"{}\")?;
			{}
		}}", f.ts, generate_deserializer(f)));
	}
	res
}

fn gen_return_match(to: &[&Property]) -> String {
	if to.len() == 1 {
		to[0].name.to_snake_case()
	} else {
		format!(
			"({})",
			to.iter()
				.map(|p| p.name.to_snake_case())
				.collect::<Vec<_>>()
				.join(", ")
		)
	}
}

fn get_property_name(e: &Event, p: &Property) -> String {
	format!(
		"{}{}",
		e.book_struct.name,
		crate::events::get_property_name(p)
	)
}

fn get_property_id(e: &Event, p: &Property, from: &Field) -> String {
	let mut ids = get_id_args(e);
	if let Some(m) = &p.modifier {
		if !ids.is_empty() {
			ids.push_str(", ");
		}
		if m == "map" || m == "array" {
			ids.push_str(&format!("{{
				let val = cmd.get_arg(\"{}\")?;
				{}
			}}", from.ts, generate_deserializer(from)));
		} else {
			panic!("Unknown modifier {}", m);
		}
	}

	if !ids.is_empty() {
		ids = format!("({})", ids);
	}
	format!("PropertyId::{}{}", get_property_name(e, p), ids)
}

fn get_property(p: &Property, name: &str) -> String {
	let type_s = get_rust_type(p);
	let type_s = type_s.replace('<', "_").replace('>', "").to_camel_case();
	format!("PropertyValue::{}({})", type_s, name)
}

fn generate_deserializer(field: &Field) -> String {
	let rust_type = field.get_rust_type("", false);
	let res = if rust_type.starts_with("Vec<") {
		crate::message_parser::vector_value_deserializer(field)
	} else {
		crate::message_parser::single_value_deserializer(field, &rust_type)
	};
	res.replace("*val", "val")
}
