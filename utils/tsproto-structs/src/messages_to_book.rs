use std::collections::HashSet;
use std::fmt::Write;
use std::str::FromStr;

use once_cell::sync::Lazy;
use serde::Deserialize;

use crate::book::{BookDeclarations, Property, Struct};
use crate::messages::{Field, Message, MessageDeclarations};
use crate::*;

pub const DATA_STR: &str =
	include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/declarations/MessagesToBook.toml"));

pub static DATA: Lazy<MessagesToBookDeclarations<'static>> = Lazy::new(|| {
	let rules: TomlStruct = toml::from_str(DATA_STR).unwrap();
	let book = &book::DATA;
	let messages = &messages::DATA;

	let mut decls: Vec<_> = rules
		.rule
		.into_iter()
		.map(|r| {
			let msg = messages.get_message(&r.from);
			let msg_fields =
				msg.attributes.iter().map(|a| messages.get_field(a)).collect::<Vec<_>>();
			let book_struct = book
				.structs
				.iter()
				.find(|s| s.name == r.to)
				.unwrap_or_else(|| panic!("Cannot find struct {}", r.to));

			let mut ev = Event {
				op: r.operation.parse().expect("Failed to parse operation"),
				id: r.id.iter().map(|s| find_field(s, &msg_fields)).collect(),
				msg,
				book_struct,
				rules: r
					.properties
					.into_iter()
					.map(|p| {
						assert!(p.is_valid());

						let find_prop = |name, book_struct: &'static Struct| -> &'static Property {
							if let Some(prop) =
								book_struct.properties.iter().find(|p| p.name == name)
							{
								return prop;
							}
							panic!(
								"No such (nested) property {} found in struct {}",
								name, book_struct.name,
							);
						};

						if p.function.is_some() {
							RuleKind::Function {
								name: p.function.unwrap(),
								to: p
									.tolist
									.unwrap()
									.into_iter()
									.map(|p| find_prop(p, book_struct))
									.collect(),
							}
						} else {
							RuleKind::Map {
								from: find_field(&p.from.unwrap(), &msg_fields),
								to: find_prop(p.to.unwrap(), book_struct),
								op: p
									.operation
									.map(|s| s.parse().expect("Invalid operation for property"))
									.unwrap_or(RuleOp::Update),
							}
						}
					})
					.collect(),
			};

			// Add attributes with the same name automatically (if they are not
			// yet there).
			let used_flds = ev
				.rules
				.iter()
				.filter_map(|f| match *f {
					RuleKind::Map { from, .. } => Some(from),
					_ => None,
				})
				.collect::<HashSet<_>>();

			let mut used_props = vec![];
			for rule in &ev.rules {
				if let RuleKind::Function { to, .. } = rule {
					for p in to {
						used_props.push(p.name.clone());
					}
				}
			}

			for fld in &msg_fields {
				if used_flds.contains(fld) {
					continue;
				}
				if let Some(prop) = book
					.get_struct(&ev.book_struct.name)
					.properties
					.iter()
					.find(|p| p.name == fld.pretty)
				{
					if used_props.contains(&prop.name) {
						continue;
					}

					ev.rules.push(RuleKind::Map { from: fld, to: prop, op: RuleOp::Update });
				}
			}

			ev
		})
		.collect();

	// InitServer is done manually
	decls.retain(|ev| ev.msg.name != "InitServer");

	MessagesToBookDeclarations { book, messages, decls }
});

#[derive(Debug)]
pub struct MessagesToBookDeclarations<'a> {
	pub book: &'a BookDeclarations,
	pub messages: &'a MessageDeclarations,
	pub decls: Vec<Event<'a>>,
}

#[derive(Debug)]
pub struct Event<'a> {
	pub op: RuleOp,
	/// Unique access tuple to get the property
	pub id: Vec<&'a Field>,
	pub msg: &'a Message,
	pub book_struct: &'a Struct,
	pub rules: Vec<RuleKind<'a>>,
}

#[derive(Debug)]
pub enum RuleKind<'a> {
	Map { from: &'a Field, to: &'a Property, op: RuleOp },
	Function { name: String, to: Vec<&'a Property> },
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum RuleOp {
	Add,
	Remove,
	Update,
}

#[derive(Clone, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct TomlStruct {
	rule: Vec<Rule>,
}

#[derive(Clone, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct Rule {
	id: Vec<String>,
	from: String,
	to: String,
	operation: String,
	#[serde(default = "Vec::new")]
	properties: Vec<RuleProperty>,
}

#[derive(Clone, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct RuleProperty {
	from: Option<String>,
	to: Option<String>,
	operation: Option<String>,

	function: Option<String>,
	tolist: Option<Vec<String>>,
}

impl Event<'_> {
	/// Fill the id of a `PropertyId`.
	///
	/// `msg` is the name of the message object.
	pub fn get_id_args(&self, msg: &str) -> String {
		let mut res = String::new();
		for f in &self.id {
			if !res.is_empty() {
				res.push_str(", ");
			}
			if !f.get_type("").unwrap().is_primitive() {
				res.push('&');
			}
			let _ = write!(res, "{}.{}", msg, f.get_rust_name());
		}
		res
	}

	/// Create a `PropertyId` from a message struct.
	///
	/// `msg` is the name of the message object.
	pub fn get_property_id(&self, p: &Property, from: &Field, msg: &str) -> String {
		let mut ids = self.get_id_args(msg);
		if let Some(m) = &p.modifier {
			if !ids.is_empty() {
				ids.push_str(", ");
			}
			if m == "map" || m == "array" || m == "set" {
				let _ = write!(ids, "{}.{}", msg, from.get_rust_name());
			} else {
				panic!("Unknown modifier {}", m);
			}
		}
		format!("PropertyId::{}{}{}", self.book_struct.name, p.get_name(), embrace(&ids))
	}
}

impl RuleProperty {
	fn is_valid(&self) -> bool {
		if self.from.is_some() {
			self.to.is_some() && self.function.is_none() && self.tolist.is_none()
		} else {
			self.from.is_none()
				&& self.to.is_none()
				&& self.operation.is_none()
				&& self.function.is_some()
				&& self.tolist.is_some()
		}
	}
}

impl FromStr for RuleOp {
	type Err = fmt::Error;
	fn from_str(s: &str) -> Result<Self> {
		if s == "add" {
			Ok(RuleOp::Add)
		} else if s == "remove" {
			Ok(RuleOp::Remove)
		} else if s == "update" {
			Ok(RuleOp::Update)
		} else {
			eprintln!("Cannot parse operation, needs to be add, remove or update");
			Err(fmt::Error)
		}
	}
}

// the in rust callable name (in PascalCase) from the field
fn find_field<'a>(name: &str, msg_fields: &[&'a Field]) -> &'a Field {
	msg_fields
		.iter()
		.find(|f| f.pretty == name)
		.unwrap_or_else(|| panic!("Cannot find field '{}'", name))
}

impl<'a> RuleKind<'a> {
	pub fn is_function(&self) -> bool { matches!(self, RuleKind::Function { .. }) }
}
