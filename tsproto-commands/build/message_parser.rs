use std::default::Default;
use itertools::Itertools;
use tsproto_structs::messages::*;
use tsproto_structs::messages;

#[derive(Template)]
#[TemplatePath = "build/MessageDeclarations.tt"]
#[derive(Debug, Clone)]
pub struct MessageDeclarations<'a>(&'a messages::MessageDeclarations);

impl Default for MessageDeclarations<'static> {
	fn default() -> Self {
		MessageDeclarations(&DATA)
	}
}
