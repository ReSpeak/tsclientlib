use std::ops::Deref;

use heck::*;
use t4rust_derive::Template;
use tsproto_structs::errors::*;
use tsproto_structs::EnumValue;
use tsproto_structs::{doc_comment, indent};

#[derive(Template)]
#[TemplatePath = "build/Errors.tt"]
#[derive(Default, Debug)]
pub struct Errors;

impl Deref for Errors {
	type Target = Vec<EnumValue>;
	fn deref(&self) -> &Self::Target { &DATA.0 }
}
