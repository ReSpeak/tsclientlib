use std::ops::Deref;

use t4rust_derive::Template;
use tsproto_structs::errors::*;
use tsproto_structs::EnumValue;
use tsproto_structs::{doc_comment, indent, to_pascal_case};

#[derive(Template)]
#[TemplatePath = "build/ErrorDeclarations.tt"]
#[derive(Default, Debug)]
pub struct Errors;

impl Deref for Errors {
	type Target = Vec<EnumValue>;
	fn deref(&self) -> &Self::Target { &DATA.0 }
}
