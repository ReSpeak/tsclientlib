use std::default::Default;
use std::ops::Deref;
use tsproto_structs::convert_type;
use tsproto_structs::book_to_messages;
use tsproto_structs::book_to_messages::*;
use tsproto_structs::messages::Message;
use tsproto_util::*;

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

/// Creates `to: from,`
fn struct_assign(r: &RuleKind, msg: &Message) -> String {
	match r {
		RuleKind::Map { from, to } => {
			let fr = to_snake_case(&from.name);
			let fr = if to.is_opt(msg) {
				format!("Some({})", fr)
			} else {
				fr
			};
			format!("{}: {},", to.get_rust_name(), fr)
		}
		RuleKind::Function { to, .. } |
		RuleKind::ArgumentFunction { to, .. } => {
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
	}
}

fn get_arguments(r: &RuleKind) -> String {
	match r {
		RuleKind::Map { .. } | RuleKind::Function { .. } =>
			format!("{}: {}", to_snake_case(r.from_name()), to_ref_type(&r.from().get_rust_type())),
		RuleKind::ArgumentFunction { from, type_s, .. } =>
			format!("{}: {}", to_snake_case(from), convert_type(type_s, true)),
	}
}
