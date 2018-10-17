use std::ops::Deref;
use tsproto_structs::permissions::*;
use *;

#[derive(Template)]
#[TemplatePath = "src/PermissionDeclarations.tt"]
#[derive(Default, Debug)]
pub struct Permissions;

impl Deref for Permissions {
	type Target = Vec<EnumValue>;
	fn deref(&self) -> &Self::Target { &DATA.0 }
}
