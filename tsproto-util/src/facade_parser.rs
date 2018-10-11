use book_parser::*;
use *;

#[derive(Template)]
#[TemplatePath = "src/FacadeDeclarations.tt"]
#[derive(Debug)]
pub struct FacadeDeclarations(pub BookDeclarations);
