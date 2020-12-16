use heck::*;
use once_cell::sync::Lazy;
use serde::Deserialize;

use crate::*;

pub const DATA_STR: &str =
	include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/declarations/Book.toml"));

pub static DATA: Lazy<BookDeclarations> = Lazy::new(|| toml::from_str(DATA_STR).unwrap());

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
	#[serde(default = "get_false")]
	pub opt: bool,
	pub id: Vec<Id>,
	pub doc: String,
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
	#[serde(default = "get_false")]
	pub opt: bool,
	#[serde(rename = "mod")]
	pub modifier: Option<String>,
	pub key: Option<String>,
}

impl Struct {
	pub fn get_type(&self) -> Result<RustType> { RustType::with_opt(&self.name, self.opt) }

	pub fn get_ids(&self, structs: &[Struct]) -> String {
		let mut res = String::new();
		for id in &self.id {
			let p = id.find_property(structs);
			if !res.is_empty() {
				res.push_str(", ");
			}
			res.push_str(&p.get_type().unwrap().to_string());
		}
		embrace(&res)
	}

	pub fn get_properties(&self, structs: &[Struct]) -> Vec<&Property> {
		self.properties.iter().filter(|p| !structs.iter().any(|s| s.name == p.type_s)).collect()
	}

	/// Get all properties, including foreign ids (own ids are listed in properties).
	pub fn get_all_properties(&self) -> impl Iterator<Item = PropId> {
		self.id.iter()
			// Only foreign ids, others are also stored in the properties
			.filter_map(move |i| if i.struct_name != self.name { Some(PropId::from(i)) }
				else { None })
			.chain(self.properties.iter().map(|p| p.into()))
	}
}

impl Property {
	pub fn get_inner_type(&self) -> Result<RustType> { RustType::with_opt(&self.type_s, self.opt) }

	pub fn get_type(&self) -> Result<RustType> {
		let key = if self.is_map() {
			Some(self.key.as_deref().ok_or_else(|| {
				eprintln!("Specified map without key");
				fmt::Error
			})?)
		} else {
			None
		};
		RustType::with(&self.type_s, self.opt, key, self.is_set(), self.is_array())
	}

	/// Gets the type as a name, used for storing it in an enum.
	pub fn get_inner_type_as_name(&self) -> Result<String> { Ok(self.get_inner_type()?.to_name()) }

	pub fn get_ids(&self, structs: &[Struct], struc: &Struct) -> String {
		let mut ids = struc.get_ids(structs);
		if !ids.is_empty() {
			ids.remove(0);
			ids.pop();
		}
		if let Some(m) = &self.modifier {
			if !ids.is_empty() {
				ids.push_str(", ");
			}
			if m == "map" {
				// The key is part of the id
				ids.push_str(self.key.as_ref().unwrap());
			} else if m == "array" || m == "set" {
				// Take the element itself as part of the id.
				// It has to be copied but most of the times it is an id itself.
				ids.push_str(&self.get_inner_type().unwrap().to_string());
			} else {
				panic!("Unknown modifier {}", m);
			}
		}
		embrace(&ids)
	}

	/// Get the name without trailing `s`.
	pub fn get_name(&self) -> &str {
		if self.modifier.is_some() && self.name.ends_with('s') {
			&self.name[..self.name.len() - 1]
		} else {
			&self.name
		}
	}

	pub fn is_array(&self) -> bool { self.modifier.as_ref().map(|s| s == "array").unwrap_or(false) }
	pub fn is_set(&self) -> bool { self.modifier.as_ref().map(|s| s == "set").unwrap_or(false) }
	pub fn is_map(&self) -> bool { self.modifier.as_ref().map(|s| s == "map").unwrap_or(false) }
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
					format!("{}_{}", id.struct_name.to_snake_case(), id.prop.to_snake_case(),)
				}
			}
		}
	}

	pub fn get_doc(&self) -> Option<&str> {
		match *self {
			PropId::Prop(p) => p.doc.as_deref(),
			PropId::Id(_) => None,
		}
	}

	pub fn get_type(&self, structs: &[Struct]) -> Result<RustType> {
		match *self {
			PropId::Prop(p) => p.get_type(),
			PropId::Id(id) => id.find_property(structs).get_type(),
		}
	}
}

impl<'a> From<&'a Property> for PropId<'a> {
	fn from(p: &'a Property) -> Self { PropId::Prop(p) }
}

impl<'a> From<&'a Id> for PropId<'a> {
	fn from(p: &'a Id) -> Self { PropId::Id(p) }
}
