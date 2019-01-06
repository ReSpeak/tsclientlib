use std::str::FromStr;

use lazy_static::lazy_static;
use serde_derive::Deserialize;

use crate::*;
use crate::book::{BookDeclarations, Property, Struct};
use crate::messages::{MessageDeclarations, Field, Message};

pub const DATA_STR: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"),
	"/../declarations/BookToMessages.toml"));

lazy_static!{
	pub static ref DATA: BookToMessagesDeclarations<'static> = {
		let rules: TomlStruct = toml::from_str(DATA_STR).unwrap();
		let book = &book::DATA;
		let messages = &messages::DATA;

		let decls: Vec<_> = rules
			.rule
			.into_iter()
			.map(|r| {
				let msg = messages.get_message(&r.to);
				let msg_fields = msg
					.attributes
					.iter()
					.map(|a| messages.get_field(a))
					.collect::<Vec<_>>();
				let book_struct = book
					.structs
					.iter()
					.find(|s| s.name == r.from)
					.unwrap_or_else(|| panic!("Cannot find struct {}", r.from));

				let find_prop = |name: &str,
							     book_struct: &'static Struct|
				 -> &'static Property {
					if let Some(prop) = book_struct
						.properties
						.iter()
						.find(|p| p.name == *name)
					{
						return prop;
					}
					panic!(
						"No such (nested) property {} found in \
						 struct",
						name
					);
				};

				// Map RuleProperty to RuleKind
				let to_rule_kind = |p: RuleProperty| {
					assert!(p.is_valid());

					if p.function.is_some() {
						let rule = RuleKind::Function {
							from: find_prop(&p.from, book_struct),
							name: p.function.unwrap(),
							to: p.tolist.unwrap()
								.into_iter()
								.map(|p| find_field(&p, &msg_fields))
								.collect(),
						};
						rule
					} else {
						RuleKind::Map {
							from: find_prop(
								&p.from,
								book_struct,
							),
							to: find_field(&p.to.unwrap(), &msg_fields),
						}
					}
				};

				let mut ev = Event {
					op: r.operation.parse().expect("Failed to parse operation"),
					ids: r.ids.into_iter().map(to_rule_kind).collect(),
					msg,
					book_struct: book_struct,
					rules: r.properties.into_iter().map(to_rule_kind).collect(),
				};

				// Add ids, which are required fields in the message.
				// The filter checks that the message is not optional.
				for field in msg_fields.iter()
					.filter(|f| msg.attributes.iter().any(|a| *a == f.map)) {
					if !ev.ids.iter().any(|i| match i {
						RuleKind::Map { to, .. } => to == field,
						RuleKind::Function { to, .. } => to.contains(field),
					}) {
						// Try to find matching property
						if let Some(prop) = book
							.get_struct(&ev.book_struct.name)
							.properties
							.iter()
							.find(|p| !p.opt && p.name == field.pretty) {
							ev.ids.push(RuleKind::Map {
								from: prop,
								to: field,
							})
						} else {
							panic!("No matching property found for field {} \
								in message {}, but it is needed", field.pretty,
								msg.name);
						}
					}
				}

				// Add properties
				for field in msg_fields.iter()
					.filter(|f| !msg.attributes.iter().any(|a| *a == f.map)) {
					if !ev.ids.iter().chain(ev.rules.iter()).any(|i| match i {
						RuleKind::Map { to, .. } => to == field,
						RuleKind::Function { to, .. } => to.contains(field),
					}) {
						// Try to find matching property
						if let Some(prop) = book
							.get_struct(&ev.book_struct.name)
							.properties
							.iter()
							.find(|p| !p.opt && p.name == field.pretty) {
							if !ev.ids.iter().chain(ev.rules.iter())
								.any(|i| i.from().name == prop.name) {
								ev.rules.push(RuleKind::Map {
									from: prop,
									to: field,
								})
							}
						}
					}
				}

				ev
			}).collect();

		BookToMessagesDeclarations {
			book,
			messages,
			decls,
		}
	};
}

#[derive(Debug)]
pub struct BookToMessagesDeclarations<'a> {
	pub book: &'a BookDeclarations,
	pub messages: &'a MessageDeclarations,
	pub decls: Vec<Event<'a>>,
}

#[derive(Debug)]
pub struct Event<'a> {
	pub op: RuleOp,
	pub msg: &'a Message,
	pub book_struct: &'a Struct,
	pub ids: Vec<RuleKind<'a>>,
	pub rules: Vec<RuleKind<'a>>,
}

#[derive(Debug)]
pub enum RuleKind<'a> {
	Map {
		from: &'a Property,
		to: &'a Field,
	},
	Function {
		from: &'a Property,
		name: String,
		to: Vec<&'a Field>,
	},
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum RuleOp {
	Add,
	Remove,
	Update,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct TomlStruct {
	rule: Vec<Rule>,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct Rule {
	from: String,
	to: String,
	operation: String,
	#[serde(default = "Vec::new")]
	ids: Vec<RuleProperty>,
	#[serde(default = "Vec::new")]
	properties: Vec<RuleProperty>,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct RuleProperty {
	from: String,
	to: Option<String>,

	function: Option<String>,
	tolist: Option<Vec<String>>,
}

impl RuleProperty {
	fn is_valid(&self) -> bool {
		if self.to.is_some() {
			self.function.is_none() && self.tolist.is_none()
		} else {
			self.to.is_none()
				&& self.function.is_some()
				&& self.tolist.is_some()
		}
	}
}

impl FromStr for RuleOp {
	type Err = String;
	fn from_str(s: &str) -> Result<Self, Self::Err> {
		if s == "add" {
			Ok(RuleOp::Add)
		} else if s == "remove" {
			Ok(RuleOp::Remove)
		} else if s == "update" {
			Ok(RuleOp::Update)
		} else {
			Err("Cannot parse operation, needs to be add, remove or update"
				.to_string())
		}
	}
}

// the in rust callable name (in PascalCase) from the field
fn find_field<'a>(name: &str, msg_fields: &[&'a Field]) -> &'a Field {
	*msg_fields
		.iter()
		.find(|f| f.pretty == name)
		.expect(&format!("Cannot find field '{}'", name))
}

impl<'a> RuleKind<'a> {
	pub fn from(&self) -> &'a Property {
		match self {
			RuleKind::Map { from, .. } |
			RuleKind::Function { from, .. } => from,
		}
	}

	pub fn is_function(&self) -> bool {
		if let RuleKind::Function { .. } = *self {
			true
		} else {
			false
		}
	}
}
