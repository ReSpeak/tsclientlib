use std::default::Default;
use tsproto_structs::messages::*;
use tsproto_structs::messages;

#[derive(Template)]
#[TemplatePath = "src/MessageDeclarations.tt"]
#[derive(Debug, Clone)]
pub struct MessageDeclarations<'a>(&'a messages::MessageDeclarations);

impl Default for MessageDeclarations<'static> {
	fn default() -> Self {
		MessageDeclarations(&DATA)
	}
}
