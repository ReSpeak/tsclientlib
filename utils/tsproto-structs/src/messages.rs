use crate::*;
use lazy_static::lazy_static;
use serde_derive::Deserialize;

pub const DATA_STR: &str = include_str!(concat!(
	env!("CARGO_MANIFEST_DIR"),
	"/declarations/Messages.toml"
));

lazy_static! {
	pub static ref DATA: MessageDeclarations =
		toml::from_str(DATA_STR).unwrap();
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct MessageDeclarations {
	pub fields: Vec<Field>,
	pub msg_group: Vec<MessageGroup>,
}

impl MessageDeclarations {
	pub fn get_message_opt(&self, name: &str) -> Option<&Message> {
		self.msg_group
			.iter()
			.flat_map(|g| g.msg.iter())
			.find(|m| m.name == name)
	}

	pub fn get_message(&self, name: &str) -> &Message {
		self.get_message_opt(name)
			.unwrap_or_else(|| panic!("Cannot find message {}", name))
	}

	pub fn get_message_group(&self, msg: &Message) -> &MessageGroup {
		for g in &self.msg_group {
			for m in &g.msg {
				if m as *const Message == msg as *const Message {
					return g;
				}
			}
		}
		panic!("Cannot find message group for message");
	}

	pub fn get_field(&self, mut map: &str) -> &Field {
		if map.ends_with('?') {
			map = &map[..map.len() - 1];
		}
		if let Some(f) = self.fields.iter().find(|f| f.map == map) {
			f
		} else {
			panic!("Cannot find field {}", map);
		}
	}
}

#[derive(Deserialize, Debug, Clone, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Field {
	/// Internal name of this declarations file to map fields to messages.
	pub map: String,
	/// The name as called by TeamSpeak in messages.
	pub ts: String,
	/// The pretty name in PascalCase. This will be used for the fields in rust.
	pub pretty: String,
	#[serde(rename = "type")]
	pub type_s: String,
	#[serde(rename = "mod")]
	pub modifier: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct MessageGroup {
	pub default: MessageGroupDefaults,
	pub msg: Vec<Message>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct MessageGroupDefaults {
	pub s2c: bool,
	pub c2s: bool,
	pub response: bool,
	pub low: bool,
	pub np: bool,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct Message {
	/// How we call this message.
	pub name: String,
	/// How TeamSpeak calls this message.
	pub notify: Option<String>,
	pub attributes: Vec<String>,
}

impl Field {
	pub fn get_rust_name(&self) -> String { to_snake_case(&self.pretty) }

	/// Takes the attribute to look if it is optional
	pub fn get_rust_type(&self, a: &str, is_ref: bool) -> String {
		let mut res = convert_type(&self.type_s, is_ref);

		if self
			.modifier
			.as_ref()
			.map(|s| s == "array")
			.unwrap_or(false)
		{
			res = format!("Vec<{}>", res);
		}
		if a.ends_with('?') {
			res = format!("Option<{}>", res);
		}
		res
	}

	/// Returns if this field is optional in the message.
	pub fn is_opt(&self, msg: &Message) -> bool {
		!msg.attributes.iter().any(|a| *a == self.map)
	}
}
