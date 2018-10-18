use std::default::Default;
use tsproto_structs::book::BookDeclarations;
use *;

#[derive(Template)]
#[TemplatePath = "src/FacadeDeclarations.tt"]
#[derive(Debug)]
pub struct FacadeDeclarations<'a>(&'a BookDeclarations);

impl Default for FacadeDeclarations<'static> {
	fn default() -> Self {
		FacadeDeclarations(&tsproto_structs::book::DATA)
	}
}
