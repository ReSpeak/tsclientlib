use tsproto_structs::book::BookDeclarations;
use *;

#[derive(Template)]
#[TemplatePath = "src/FacadeDeclarations.tt"]
#[derive(Debug)]
pub struct FacadeDeclarations(pub BookDeclarations);
