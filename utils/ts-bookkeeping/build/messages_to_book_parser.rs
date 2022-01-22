use std::collections::HashMap;
use std::default::Default;
use std::ops::Deref;

use heck::*;
use t4rust_derive::Template;
use tsproto_structs::book::{PropId, Property};
use tsproto_structs::messages::Field;
use tsproto_structs::messages_to_book::*;
use tsproto_structs::*;

#[derive(Template)]
#[TemplatePath = "build/MessagesToBook.tt"]
#[derive(Debug)]
pub struct MessagesToBookDeclarations<'a>(&'a messages_to_book::MessagesToBookDeclarations<'a>);

impl<'a> Deref for MessagesToBookDeclarations<'a> {
	type Target = messages_to_book::MessagesToBookDeclarations<'a>;
	fn deref(&self) -> &Self::Target { self.0 }
}

impl Default for MessagesToBookDeclarations<'static> {
	fn default() -> Self { MessagesToBookDeclarations(&DATA) }
}

fn get_property(p: &Property, name: &str) -> String {
	format!("PropertyValue::{}({})", p.get_inner_type_as_name().unwrap(), name)
}
