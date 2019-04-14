use std::default::Default;

use itertools::Itertools;
use t4rust_derive::Template;
use tsproto_structs::indent;
use tsproto_structs::messages::*;
use tsproto_structs::messages;

#[derive(Template)]
#[TemplatePath = "build/MessageDeclarations.tt"]
#[derive(Debug, Clone)]
pub struct MessageDeclarations<'a>(pub &'a messages::MessageDeclarations);

impl MessageDeclarations<'static> {
	pub fn s2c() -> messages::MessageDeclarations {
		let mut res = DATA.clone();
		res.msg_group.retain(|g| g.default.s2c);
		res
	}
	pub fn c2s() -> messages::MessageDeclarations {
		let mut res = DATA.clone();
		res.msg_group.retain(|g| g.default.c2s);
		res
	}
}

impl Default for MessageDeclarations<'static> {
	fn default() -> Self {
		MessageDeclarations(&DATA)
	}

}
