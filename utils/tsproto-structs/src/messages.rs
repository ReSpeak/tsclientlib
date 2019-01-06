use lazy_static::lazy_static;
use serde_derive::Deserialize;
use crate::*;

pub const DATA_STR: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"),
	"/../declarations/Messages.toml"));

lazy_static!{
	pub static ref DATA: MessageDeclarations = toml::from_str(DATA_STR).unwrap();
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
		self.get_message_opt(name).unwrap_or_else(||
			panic!("Cannot find message {}", name))
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
	pub fn get_rust_name(&self) -> String {
		to_snake_case(&self.pretty)
	}

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

/// If `is_ref` is `true`, you get e.g. `&str` instead of `String`.
pub fn convert_type(t: &str, is_ref: bool) -> String {
	if t.ends_with("[]") {
		let inner = &t[..(t.len() - 2)];
		if is_ref {
			return format!("&[{}]", convert_type(inner, is_ref));
		} else {
			return format!("Vec<{}>", convert_type(inner, is_ref));
		}
	}
	if t.ends_with("T") {
		return convert_type(&t[..(t.len() - 1)], is_ref);
	}

	if t == "str" || t == "string" {
		if is_ref {
			String::from("&str")
		} else {
			String::from("String")
		}
	} else if t == "byte" {
		String::from("u8")
	} else if t == "ushort" {
		String::from("u16")
	} else if t == "int" {
		String::from("i32")
	} else if t == "uint" {
		String::from("u32")
	} else if t == "float" {
		String::from("f32")
	} else if t == "long" {
		String::from("i64")
	} else if t == "ulong" {
		String::from("u64")
	} else if t == "ushort" {
		String::from("u16")
	} else if t == "DateTime" {
		String::from("DateTime<Utc>")
	} else if t.starts_with("Duration") {
		String::from("Duration")
	} else if t == "ClientUid" {
		if is_ref {
			String::from("UidRef")
		} else {
			String::from("Uid")
		}
	} else if t == "Ts3ErrorCode" {
		String::from("Error")
	} else if t == "PermissionId" {
		String::from("Permission")
	} else if t == "Uid" && is_ref {
		String::from("UidRef")
	} else {
		t.into()
	}
}
