use std::default::Default;
use std::ops::Deref;
use tsproto_structs::book::{PropId, Property};
use tsproto_structs::messages::Field;
use tsproto_structs::messages_to_book;
use tsproto_structs::messages_to_book::*;
use *;

#[derive(Template)]
#[TemplatePath = "src/MessagesToBook.tt"]
#[derive(Debug)]
pub struct MessagesToBookDeclarations<'a>(&'a messages_to_book::MessagesToBookDeclarations<'a>);

impl<'a> Deref for MessagesToBookDeclarations<'a> {
	type Target = messages_to_book::MessagesToBookDeclarations<'a>;
	fn deref(&self) -> &Self::Target { &self.0 }
}

impl Default for MessagesToBookDeclarations<'static> {
	fn default() -> Self {
		MessagesToBookDeclarations(&DATA)
	}
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

fn get_notification_field(from: &Field) -> String {
	let rust_type = from.get_rust_type("", false);
	if rust_type == "String"
		|| rust_type == "Uid"
		|| rust_type.starts_with("Vec<")
	{
		format!("{}.clone()", from.get_rust_name())
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
