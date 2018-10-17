use std::ops::Deref;
use tsproto_structs::versions::*;

#[derive(Template)]
#[TemplatePath = "src/VersionDeclarations.tt"]
#[derive(Default, Debug)]
pub struct Versions;

impl Deref for Versions {
	type Target = Vec<Version>;
	fn deref(&self) -> &Self::Target { &DATA.0 }
}
