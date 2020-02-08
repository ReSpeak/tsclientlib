use std::default::Default;
use std::ops::Deref;

use heck::*;
use t4rust_derive::Template;
use tsproto_structs::book_to_messages::*;
use tsproto_structs::*;

#[derive(Template)]
#[TemplatePath = "build/BookToMessages.tt"]
#[derive(Debug)]
pub struct BookToMessagesDeclarations<'a>(
	&'a book_to_messages::BookToMessagesDeclarations<'a>,
);

impl<'a> Deref for BookToMessagesDeclarations<'a> {
	type Target = book_to_messages::BookToMessagesDeclarations<'a>;
	fn deref(&self) -> &Self::Target { &self.0 }
}

impl Default for BookToMessagesDeclarations<'static> {
	fn default() -> Self { BookToMessagesDeclarations(&DATA) }
}

fn to_ref_type(s: &str) -> String {
	if s == "String" { "&str".into() } else { s.into() }
}

fn get_arguments(r: &RuleKind) -> String {
	match r {
		RuleKind::Map { .. } | RuleKind::Function { .. } => format!(
			"{}: {}",
			r.from_name().to_snake_case(),
			to_ref_type(&r.from().get_rust_type()),
		),
		RuleKind::ArgumentMap { from, to } => format!(
			"{}: {}",
			from.to_snake_case(),
			convert_type(&to.type_s, true),
		),
		RuleKind::ArgumentFunction { from, type_s, .. } => {
			format!("{}: {}", from.to_snake_case(), convert_type(type_s, true))
		}
	}
}

fn get_all_arguments<'a>(
	e: &'a Event<'a>,
	r: Option<&'a RuleKind<'a>>,
) -> String
{
	let mut args = String::new();
	for r in e.ids.iter().chain(r.iter().cloned()) {
		match r {
			RuleKind::ArgumentMap { .. }
			| RuleKind::ArgumentFunction { .. } => {
				let arg = get_arguments(r);
				if !arg.is_empty() {
					args.push_str(", ");
					args.push_str(&arg);
				}
			}
			_ => {}
		}
	}
	args
}

fn get_arguments_use(r: &RuleKind) -> String {
	match r {
		RuleKind::Map { .. } | RuleKind::Function { .. } => {
			format!("{}", r.from_name().to_snake_case(),)
		}
		RuleKind::ArgumentMap { from, .. } => {
			format!("{}", from.to_snake_case(),)
		}
		RuleKind::ArgumentFunction { from, .. } => {
			format!("{}", from.to_snake_case())
		}
	}
}

fn get_all_arguments_use<'a>(
	e: &'a Event<'a>,
	r: Option<&'a RuleKind<'a>>,
) -> String
{
	let mut args = String::new();
	for r in e.ids.iter().chain(r.iter().cloned()) {
		match r {
			RuleKind::ArgumentMap { .. }
			| RuleKind::ArgumentFunction { .. } => {
				let arg = get_arguments_use(r);
				if !arg.is_empty() {
					args.push_str(&arg);
					args.push_str(", ");
				}
			}
			_ => {}
		}
	}
	args
}
