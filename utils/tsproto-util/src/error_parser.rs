use std::ops::Deref;
use tsproto_structs::errors::*;
use *;

#[derive(Template)]
#[TemplatePath = "src/ErrorDeclarations.tt"]
#[derive(Default, Debug)]
pub struct Errors;

impl Deref for Errors {
	type Target = Vec<EnumValue>;
	fn deref(&self) -> &Self::Target { &DATA.0 }
}
