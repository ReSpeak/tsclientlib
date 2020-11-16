use std::default::Default;

use t4rust_derive::Template;
use tsproto_structs::*;

#[derive(Template)]
#[TemplatePath = "build/BookDeclarations.tt"]
#[derive(Debug)]
pub struct BookDeclarations<'a>(pub &'a book::BookDeclarations);

impl Default for BookDeclarations<'static> {
	fn default() -> Self { BookDeclarations(&tsproto_structs::book::DATA) }
}
