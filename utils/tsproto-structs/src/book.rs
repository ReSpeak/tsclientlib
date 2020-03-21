use heck::*;
use lazy_static::lazy_static;
use serde::Deserialize;

use crate::*;

pub const DATA_STR: &str = include_str!(concat!(
	env!("CARGO_MANIFEST_DIR"),
	"/declarations/Book.toml"
));

lazy_static! {
	pub static ref DATA: BookDeclarations = toml::from_str(DATA_STR).unwrap();
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct BookDeclarations {
	#[serde(rename = "struct")]
	pub structs: Vec<Struct>,
}

impl BookDeclarations {
	pub fn get_struct(&self, name: &str) -> &Struct {
		if let Some(s) = self.structs.iter().find(|s| s.name == name) {
			s
		} else {
			panic!("Cannot find bookkeeping struct {}", name);
		}
	}
}

#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Accessors {
	pub get: bool,
	pub set: bool,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Id {
	#[serde(rename = "struct")]
	pub struct_name: String,
	pub prop: String,
}

impl Id {
	pub fn find_property<'a>(&self, structs: &'a [Struct]) -> &'a Property {
		// Find struct
		for s in structs {
			if s.name == self.struct_name {
				// Find property
				for p in &s.properties {
					if p.name == self.prop {
						return p;
					}
				}
			}
		}
		panic!("Cannot find struct {} of id", self.struct_name);
	}
}

#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Struct {
	pub name: String,
	pub id: Vec<Id>,
	pub doc: String,
	pub accessor: Accessors,
	pub properties: Vec<Property>,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Property {
	/// The name of this property (in PascalCase) which can be called from rust
	/// when generated.
	pub name: String,
	/// The rust declaration type.
	#[serde(rename = "type")]
	pub type_s: String,
	pub doc: Option<String>,
	pub get: Option<bool>,
	pub set: Option<bool>,
	#[serde(default = "get_false")]
	pub opt: bool,
	#[serde(rename = "mod")]
	pub modifier: Option<String>,
	pub key: Option<String>,
}

impl Property {
	pub fn get_get(&self, struc: &Struct) -> bool {
		self.get.unwrap_or_else(|| struc.accessor.get)
	}
	pub fn get_set(&self, struc: &Struct) -> bool {
		self.set.unwrap_or_else(|| struc.accessor.set)
	}
	pub fn get_rust_type(&self) -> String {
		let mut res = convert_type(&self.type_s, false);

		if self.is_array() {
			res = format!("Vec<{}>", res);
		} else if self.is_map() {
			let key = self.key.as_ref().expect("Specified map without key");
			res = format!("HashMap<{}, {}>", key, res);
		}
		if self.opt {
			res = format!("Option<{}>", res);
		}
		res
	}

	pub fn is_array(&self) -> bool {
		self.modifier.as_ref().map(|s| s == "array").unwrap_or(false)
	}
	pub fn is_map(&self) -> bool {
		self.modifier.as_ref().map(|s| s == "map").unwrap_or(false)
	}

	pub fn get_as_ref(&self) -> String {
		let res = self.get_rust_type();

		let append;
		if res.contains("&") || res.contains("Uid") {
			if self.opt {
				append = ".as_ref().map(|f| f.as_ref())";
			} else if self.is_array() {
				append = ".clone()";
			} else {
				append = ".as_ref()";
			}
		} else if self.is_array() {
			append = ".clone()";
		} else {
			append = "";
		}
		append.into()
	}
}

pub enum PropId<'a> {
	Prop(&'a Property),
	Id(&'a Id),
}

impl<'a> PropId<'a> {
	pub fn get_attr_name(&self, struc: &Struct) -> String {
		match *self {
			PropId::Prop(p) => p.name.to_snake_case(),
			PropId::Id(id) => {
				if struc.name == id.struct_name {
					id.prop.to_snake_case()
				} else {
					format!(
						"{}_{}",
						id.struct_name.to_snake_case(),
						id.prop.to_snake_case(),
					)
				}
			}
		}
	}

	pub fn get_doc(&self) -> Option<&str> {
		match *self {
			PropId::Prop(p) => p.doc.as_ref().map(|s| s.as_str()),
			PropId::Id(_) => None,
		}
	}

	pub fn get_rust_type(&self, structs: &[Struct]) -> String {
		match *self {
			PropId::Prop(p) => p.get_rust_type(),
			PropId::Id(id) => id.find_property(structs).get_rust_type(),
		}
	}
}

impl<'a> From<&'a Property> for PropId<'a> {
	fn from(p: &'a Property) -> Self { PropId::Prop(p) }
}

impl<'a> From<&'a Id> for PropId<'a> {
	fn from(p: &'a Id) -> Self { PropId::Id(p) }
}
