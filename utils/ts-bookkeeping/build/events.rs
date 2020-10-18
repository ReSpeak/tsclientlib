use std::collections::HashSet;
use std::default::Default;
use std::ops::Deref;

use t4rust_derive::Template;
use tsproto_structs::book::*;
use tsproto_structs::messages_to_book::{self, MessagesToBookDeclarations};

#[derive(Template)]
#[TemplatePath = "build/Events.tt"]
#[derive(Debug)]
pub struct EventDeclarations<'a>(&'a BookDeclarations, &'a MessagesToBookDeclarations<'a>);

impl<'a> Deref for EventDeclarations<'a> {
	type Target = BookDeclarations;
	fn deref(&self) -> &Self::Target { &self.0 }
}

impl Default for EventDeclarations<'static> {
	fn default() -> Self { EventDeclarations(&DATA, &messages_to_book::DATA) }
}
