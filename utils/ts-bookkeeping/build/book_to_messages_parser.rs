use std::default::Default;
use std::ops::Deref;

use heck::*;
use t4rust_derive::Template;
use tsproto_structs::book_to_messages::*;
use tsproto_structs::messages::Field;
use tsproto_structs::*;

#[derive(Template)]
#[TemplatePath = "build/BookToMessages.tt"]
#[derive(Debug)]
pub struct BookToMessagesDeclarations<'a>(&'a book_to_messages::BookToMessagesDeclarations<'a>);

impl<'a> Deref for BookToMessagesDeclarations<'a> {
	type Target = book_to_messages::BookToMessagesDeclarations<'a>;
	fn deref(&self) -> &Self::Target { self.0 }
}

impl Default for BookToMessagesDeclarations<'static> {
	fn default() -> Self { BookToMessagesDeclarations(&DATA) }
}

fn get_to_list(to: &[&Field]) -> String {
	let mut res = String::new();
	if to.len() > 1 {
		res.push('(');
	}

	let mut first = true;
	for t in to {
		if !first {
			res.push_str(", ");
		} else {
			first = false;
		}
		res.push_str(&t.get_rust_name());
	}

	if to.len() > 1 {
		res.push(')');
	}
	res
}

/// The prefix is written before from, if from is a mapped argument
fn rule_has_to(r: &RuleKind, field: &Field) -> bool {
	match r {
		RuleKind::Map { to, .. } | RuleKind::ArgumentMap { to, .. } => to == &field,
		RuleKind::ArgumentFunction { to, .. } | RuleKind::Function { to, .. } => {
			to.contains(&field)
		}
	}
}

/// Finds a matching rule in either the event ids or the given rule.
fn find_rule<'a>(
	e: &'a Event, r: Option<&'a RuleKind>, field: &Field,
) -> (bool, Option<&'a RuleKind<'a>>) {
	if let Some(r) = r {
		if rule_has_to(r, field) {
			return (true, Some(r));
		}
	}
	for r in &e.ids {
		if rule_has_to(r, field) {
			return (false, Some(r));
		}
	}
	(false, None)
}
