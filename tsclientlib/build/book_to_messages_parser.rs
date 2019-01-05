use std::default::Default;
use std::ops::Deref;
use tsproto_structs::book::Property;
use tsproto_structs::messages::Field;
use tsproto_structs::book_to_messages;
use tsproto_structs::book_to_messages::*;
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
