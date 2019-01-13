use std::default::Default;
use std::ops::Deref;

use tsproto_structs::book::{PropId, Property};
use tsproto_structs::messages::{Field, Message};
use tsproto_structs::messages_to_book;
use tsproto_structs::messages_to_book::*;
use tsproto_util::*;

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
		res.push_str("cmd.");
		res.push_str(&f.get_rust_name());
	}
	res
}

fn get_notification_field(from: &Field, msg: &Message) -> String {
	let rust_type = from.get_rust_type("", false);
	if rust_type == "String" {
		if from.is_opt(msg) {
			format!("{}.map(|s| s.into())", from.get_rust_name())
		} else {
			format!("{}.into()", from.get_rust_name())
		}
	} else if rust_type.starts_with("Vec<") {
		format!("{}.clone()", from.get_rust_name())
	} else if rust_type == "Uid" {
		format!("{}.clone().into()", from.get_rust_name())
	} else {
		from.get_rust_name().clone()
	}
}

fn gen_return_match(to: &[&Property]) -> String {
	if to.len() == 1 {
		to_snake_case(&to[0].name)
	} else {
		format!(
			"({})",
			join(to.iter().map(|p| to_snake_case(&p.name)), ", ")
		)
	}
}

fn try_result(s: &str) -> &'static str {
	match s {
		"get_mut_client" | "get_mut_channel" | "add_connection_client_data" => {
			"?"
		}
		_ => "",
	}
}

fn get_property_name(e: &Event, p: &Property) -> String {
	format!("{}{}", e.book_struct.name, crate::events::get_property_name(p))
}

fn get_property_id(e: &Event, p: &Property, from: &Field) -> String {
	let mut ids = get_id_args(e);
	if let Some(m) = &p.modifier {
		if !ids.is_empty() {
			ids.push_str(", ");
		}
		if m == "map" {
			ids.push_str(&format!("cmd.{}", from.get_rust_name()));
		} else if m == "array" {
			ids.push_str(&format!("cmd.{}", from.get_rust_name()));
		} else {
			panic!("Unknown modifier {}", m);
		}
	}

	if !ids.is_empty() {
		ids = format!("({})", ids);
	}
	format!("PropertyId::{}{}", get_property_name(e, p), ids)
}

fn get_property(e: &Event, p: &Property, name: &str) -> String {
	format!("Property::{}({})", get_property_name(e, p), name)
}
