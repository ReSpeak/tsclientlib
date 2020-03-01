use std::ops::Deref;

use heck::*;
use t4rust_derive::Template;
use tsproto_structs::enums;
use tsproto_structs::enums::*;
use tsproto_structs::{doc_comment, indent};

#[derive(Template)]
#[TemplatePath = "build/Enums.tt"]
#[derive(Default, Debug)]
pub struct Enums;

impl Deref for Enums {
	type Target = enums::Enums;
	fn deref(&self) -> &Self::Target { &DATA }
}
