use std::default::Default;
use std::fmt;
use std::ops::Deref;

use t4rust_derive::Template;
use tsproto_structs::book_to_messages::*;
use tsproto_structs::messages::{Field, Message};
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
	if s == "String" {
		"&str".into()
	} else {
		s.into()
	}
}

/// Creates `to: from,`.
///
/// The prefix is written before from, if from is a mapped argument
fn struct_assign(r: &RuleKind, msg: &Message, prefix: &str) -> String {
	match r {
		RuleKind::Map { to, .. } | RuleKind::ArgumentMap { to, .. } => {
			let fr = to_snake_case(r.from_name());
			let fr = if to.is_opt(msg) {
				format!("Some({})", fr)
			} else {
				fr
			};
			if let RuleKind::Map { .. } = r {
				format!("{}: {}{},", to.get_rust_name(), prefix, fr)
			} else {
				format!("{}: {},", to.get_rust_name(), fr)
			}
		}
		RuleKind::Function { to, .. } => {
			let mut res = String::new();
			for to in to {
				let name = to.get_rust_name();
				if to.is_opt(msg) {
					res.push_str(&format!("{}: Some({}),", name, name));
				} else {
					res.push_str(&name);
					res.push(',');
				}
			}
			res
		}
		RuleKind::ArgumentFunction { to, .. } => {
			let mut res = String::new();
			for to in to {
				res.push_str(&to.get_rust_name());
				res.push(',');
			}
			res
		}
	}
}

fn get_arguments(r: &RuleKind) -> String {
	match r {
		RuleKind::Map { .. } | RuleKind::Function { .. } => format!(
			"{}: {}",
			to_snake_case(r.from_name()),
			to_ref_type(&r.from().get_rust_type())
		),
		RuleKind::ArgumentMap { from, to } => format!(
			"{}: {}",
			to_snake_case(from),
			convert_type(&to.type_s, true)
		),
		RuleKind::ArgumentFunction { from, type_s, .. } => {
			format!("{}: {}", to_snake_case(from), convert_type(type_s, true))
		}
	}
}
