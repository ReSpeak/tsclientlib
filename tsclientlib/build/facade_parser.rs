use std::default::Default;

use t4rust_derive::Template;
use tsproto_structs::*;
use tsproto_structs::book::*;

#[derive(Template)]
#[TemplatePath = "build/FacadeDeclarations.tt"]
#[derive(Debug)]
pub struct FacadeDeclarations<'a>(&'a BookDeclarations);

impl Default for FacadeDeclarations<'static> {
	fn default() -> Self { FacadeDeclarations(&tsproto_structs::book::DATA) }
}
