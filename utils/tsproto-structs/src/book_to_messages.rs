use std::str::FromStr;

use heck::*;
use lazy_static::lazy_static;
use serde::Deserialize;

use crate::book::{BookDeclarations, Property, Struct};
use crate::messages::{Field, Message, MessageDeclarations};
use crate::*;

pub const DATA_STR: &str =
	include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/declarations/BookToMessages.toml"));

lazy_static! {
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

				let find_prop = |name: &str, book_struct: &'static Struct|
				 -> Option<&'static Property> {
					if let Some(prop) = book_struct
						.properties
						.iter()
						.find(|p| p.name == *name)
					{
						Some(prop)
					} else {
						None
					}
				};

				// Map RuleProperty to RuleKind
				let to_rule_kind = |p: RuleProperty| {
					p.assert_valid();

					if p.function.is_some() {
						if p.type_s.is_some() {
							RuleKind::ArgumentFunction {
								type_s: p.type_s.unwrap(),
								from: p.from.unwrap(),
								name: p.function.unwrap(),
								to: p.tolist.unwrap()
									.into_iter()
									.map(|p| find_field(&p, &msg_fields))
									.collect(),
							}
						} else {
							RuleKind::Function {
								from: p.from.as_ref().map(|p|
									find_prop(p, book_struct)
									.unwrap_or_else(|| panic!("No such (nested) \
										property {} found in struct", p))),
								name: p.function.unwrap(),
								to: p.tolist.unwrap()
									.into_iter()
									.map(|p| find_field(&p, &msg_fields))
									.collect(),
							}
						}
					} else if let Some(prop) = find_prop(
						p.from.as_ref().unwrap(),
						book_struct,
					) {
						RuleKind::Map {
							from: prop,
							to: find_field(&p.to.unwrap(), &msg_fields),
						}
					} else {
						RuleKind::ArgumentMap {
							from: p.from.unwrap(),
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
						RuleKind::ArgumentMap { to, .. } => to == field,
						RuleKind::Function { to, .. } |
						RuleKind::ArgumentFunction { to, .. } => to.contains(field),
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
						}
						// The property may be in the properties
					}
				}

				// Add properties
				for field in msg_fields.iter()
					.filter(|f| !msg.attributes.iter().any(|a| *a == f.map)) {
					if !ev.ids.iter().chain(ev.rules.iter()).any(|i| match i {
						RuleKind::Map { to, .. } => to == field,
						RuleKind::ArgumentMap { to, .. } => to == field,
						RuleKind::Function { to, .. } |
						RuleKind::ArgumentFunction { to, .. } => to.contains(field),
					}) {
						// We ignore that properties are set as option. In all current cases, it
						// makes no sense to set them to `None`, so we handle them the same way as
						// non-optional properties.
						if let Some(prop) = book
							.get_struct(&ev.book_struct.name)
							.properties
							.iter()
							.find(|p| p.name == field.pretty) {
							if !ev.ids.iter().chain(ev.rules.iter())
								.any(|i| i.from_name() == prop.name) {
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
	Map { from: &'a Property, to: &'a Field },
	ArgumentMap { from: String, to: &'a Field },
	Function { from: Option<&'a Property>, name: String, to: Vec<&'a Field> },
	ArgumentFunction { from: String, type_s: String, name: String, to: Vec<&'a Field> },
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

#[derive(Clone, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct Rule {
	from: String,
	to: String,
	operation: String,
	#[serde(default = "Vec::new")]
	ids: Vec<RuleProperty>,
	#[serde(default = "Vec::new")]
	properties: Vec<RuleProperty>,
}

#[derive(Clone, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct RuleProperty {
	from: Option<String>,
	to: Option<String>,

	#[serde(rename = "type")]
	type_s: Option<String>,
	function: Option<String>,
	tolist: Option<Vec<String>>,
}

impl RuleProperty {
	fn assert_valid(&self) {
		if let Some(to) = &self.to {
			assert!(self.from.is_some(), "to-property '{}' is invalid. It needs a 'from'", to);
			assert!(
				self.function.is_none(),
				"to-property '{}' is invalid. It must not have a 'function'",
				to
			);
			assert!(
				self.tolist.is_none(),
				"to-property '{}' is invalid. It must not have a 'tolist'",
				to
			);
			assert!(
				self.type_s.is_none(),
				"to-property '{}' is invalid. It must not have a 'type'",
				to
			);
		} else {
			if let Some(fun) = &self.function {
				assert!(
					self.tolist.is_some(),
					"function-property '{}' is invalid. It needs 'tolist'",
					fun
				);
				assert!(
					self.type_s.is_none() || self.from.is_some(),
					"function-property '{}' is invalid. If the type ({:?}) is set, from must be \
					 set too",
					fun,
					self.type_s
				);
			} else {
				panic!(
					"Property is invalid. It needs either a 'to' or 'tolist'+'function'.Info: \
					 tolist={:?} type={:?} from={:?}",
					self.tolist, self.type_s, self.from
				);
			}
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
			Err("Cannot parse operation, needs to be add, remove or update".to_string())
		}
	}
}

// the in rust callable name (in PascalCase) from the field
fn find_field<'a>(name: &str, msg_fields: &[&'a Field]) -> &'a Field {
	*msg_fields
		.iter()
		.find(|f| f.pretty == name)
		.unwrap_or_else(|| panic!("Cannot find field '{}'", name))
}

impl<'a> RuleKind<'a> {
	pub fn from_name(&'a self) -> &'a str {
		match self {
			RuleKind::Map { from, .. } => &from.name,
			RuleKind::ArgumentMap { from, .. } => &from,
			RuleKind::Function { from, name, .. } => {
				&from.unwrap_or_else(|| panic!("From not set for function {}", name)).name
			}
			RuleKind::ArgumentFunction { from, .. } => &from,
		}
	}

	pub fn from_name_singular(&'a self) -> &'a str {
		let name = self.from_name();
		if name.ends_with('s') { &name[..name.len() - 1] } else { name }
	}

	pub fn from(&self) -> &'a Property {
		match self {
			RuleKind::Map { from, .. } => from,
			RuleKind::Function { from, name, .. } => {
				from.unwrap_or_else(|| panic!("From not set for function {}", name))
			}
			RuleKind::ArgumentMap { .. } | RuleKind::ArgumentFunction { .. } => {
				panic!("From is not a property for argument functions")
			}
		}
	}

	pub fn is_function(&self) -> bool {
		if let RuleKind::Function { .. } = *self {
			true
		} else if let RuleKind::ArgumentFunction { .. } = *self {
			true
		} else {
			false
		}
	}

	pub fn get_type(&self) -> String {
		match self {
			RuleKind::Map { .. } | RuleKind::Function { .. } => self.from().get_rust_type(true),
			RuleKind::ArgumentMap { to, .. } => convert_type(&to.type_s, true),
			RuleKind::ArgumentFunction { type_s, .. } => convert_type(type_s, true),
		}
	}

	pub fn get_type_no_option(&self) -> String {
		match self {
			RuleKind::Map { .. } => {
				// TODO: MILD HACK !!!!
				let mut rust_type = self.from().clone();
				rust_type.opt = false;
				rust_type.get_rust_type(true)
			}
			_ => self.get_type(),
		}
	}

	pub fn get_argument(&self) -> String {
		format!("{}: {}", self.from_name().to_snake_case(), self.get_type())
	}

	pub fn get_argument_no_option(&self) -> String {
		format!("{}: {}", self.from_name().to_snake_case(), self.get_type_no_option())
	}
}

impl<'a> Event<'a> {
	/// The name of the change, could be a keyword.
	pub fn get_small_name(&self) -> String { self.msg.name.replace(&self.book_struct.name, "") }

	/// The small name, not a keyword
	pub fn get_change_name(&self) -> String {
		let small_change_name = self.get_small_name();
		if small_change_name == "Move" { self.msg.name.clone() } else { small_change_name }
	}
}
