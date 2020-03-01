use std::collections::HashSet;
use std::default::Default;

use itertools::Itertools;
use t4rust_derive::Template;
use tsproto_structs::indent;
use tsproto_structs::messages;
use tsproto_structs::messages::*;

#[derive(Template)]
#[TemplatePath = "build/MessageDeclarations.tt"]
#[derive(Debug, Clone)]
pub struct MessageDeclarations<'a>(pub &'a messages::MessageDeclarations);

impl MessageDeclarations<'static> {
	pub fn s2c() -> messages::MessageDeclarations {
		let mut res = DATA.clone();
		res.msg_group.retain(|g| g.default.s2c);

		// All messages that do not occur in M2B declarations
		let not_needed: HashSet<&str> = tsproto_structs::messages_to_book::DATA
			.decls
			.iter()
			.map(|e| e.msg.name.as_str())
			.collect();
		for g in &mut res.msg_group {
			g.msg.retain(|msg| !not_needed.contains(msg.name.as_str()))
		}
		res
	}

	pub fn c2s() -> messages::MessageDeclarations {
		let mut res = DATA.clone();
		res.msg_group.retain(|g| g.default.c2s);

		// All messages that do not occur in B2M declarations
		let not_needed: HashSet<&str> = tsproto_structs::book_to_messages::DATA
			.decls
			.iter()
			.map(|e| e.msg.name.as_str())
			.collect();
		for g in &mut res.msg_group {
			g.msg.retain(|msg| !not_needed.contains(msg.name.as_str()))
		}
		res
	}
}

impl Default for MessageDeclarations<'static> {
	fn default() -> Self { MessageDeclarations(&DATA) }
}

pub fn generate_deserializer(field: &Field) -> String {
	let rust_type = field.get_rust_type("", true).replace("UidRef", "Uid");
	if rust_type.starts_with("Vec<") {
		vector_value_deserializer(field)
	} else {
		single_value_deserializer(field, &rust_type)
	}
}

pub fn single_value_deserializer(field: &Field, rust_type: &str) -> String {
	let res = match rust_type {
		"i8" | "u8" | "i16" | "u16" | "i32" | "u32" | "i64" | "u64" => format!(
			"val.parse().map_err(|e| ParseError::ParseInt {{
				arg: \"{}\",
				value: val.to_string(),
				error: e,
			}})?",
			field.pretty
		),
		"f32" | "f64" => format!(
			"val.parse().map_err(|e| ParseError::ParseFloat {{
				arg: \"{}\",
				value: val.to_string(),
				error: e,
			}})?",
			field.pretty
		),
		"bool" => format!(
			"match *val {{ \"0\" => false, \"1\" => true, _ => \
			 Err(ParseError::ParseBool {{
				arg: \"{}\",
				value: val.to_string(),
			}})? }}",
			field.pretty
		),
		"Uid" => format!("Uid(base64::decode(val).map_err(|e| ParseError::ParseUid {{
				arg: \"{}\",
				value: val.to_string(),
				error: e,
			}})?)", field.pretty),
		"&str" => "val".into(),
		"String" => "val.to_string()".into(),
		"IconHash" => format!(
			"IconHash(if val.starts_with('-') {{
			val.parse::<i32>().map(|i| i as u32)
		}} else {{
			val.parse::<u64>().map(|i| i as u32)
		}}.map_err(|e| ParseError::ParseInt {{
			arg: \"{}\",
			value: val.to_string(),
			error: e,
		}})?)",
			field.pretty
		),
		"ClientId" | "ClientDbId" | "ChannelId" | "ServerGroupId"
		| "ChannelGroupId" => format!(
			"{}(val.parse().map_err(|e| ParseError::ParseInt {{
				arg: \"{}\",
				value: val.to_string(),
				error: e,
			}})?)",
			rust_type, field.pretty
		),
		"IpAddr" | "SocketAddr" => format!(
			"val.parse().map_err(|e| ParseError::ParseAddr {{
				arg: \"{}\",
				value: val.to_string(),
				error: e,
			}})?",
			field.pretty
		),
		"TextMessageTargetMode"
		| "HostMessageMode"
		| "HostBannerMode"
		| "LicenseType"
		| "LogLevel"
		| "Codec"
		| "CodecEncryptionMode"
		| "Reason"
		| "ClientType"
		| "GroupNamingMode"
		| "GroupType"
		| "Permission"
		| "PermissionType"
		| "TokenType"
		| "PluginTargetMode"
		| "Error" => format!(
			"{}::from_u32(val.parse().map_err(|e| ParseError::ParseInt {{
				arg: \"{}\",
				value: val.to_string(),
				error: e,
			}})?).ok_or(ParseError::InvalidValue {{
				arg: \"{1}\",
				value: val.to_string(),
				}})?",
			rust_type, field.pretty
		),
		"ChannelPermissionHint" | "ClientPermissionHint" => format!(
			"{}::from_bits(val.parse().map_err(|e| ParseError::ParseInt {{
				arg: \"{}\",
				value: val.to_string(),
				error: e,
			}})?).ok_or(ParseError::InvalidValue {{
				arg: \"{1}\",
				value: val.to_string(),
				}})?",
			rust_type, field.pretty
		),
		"Duration" => {
			if field.type_s == "DurationSeconds" {
				format!(
					"let val = val.parse::<i64>().map_err(|e| \
					 ParseError::ParseInt {{
					arg: \"{}\",
					value: val.to_string(),
					error: e,
				}})?;
				if let Some(_) = val.checked_mul(1000) {{ Duration::seconds(val) }}
				else {{ Err(ParseError::InvalidValue {{
					arg: \"{0}\",
					value: val.to_string(),
					}})? }}",
					field.pretty
				)
			} else if field.type_s == "DurationMilliseconds" {
				format!(
					"Duration::milliseconds(val.parse::<i64>().map_err(|e| \
					 ParseError::ParseInt {{
					arg: \"{}\",
					value: val.to_string(),
					error: e,
				}})?)",
					field.pretty
				)
			} else {
				panic!("Unknown original time type {} found.", field.type_s);
			}
		}
		"OffsetDateTime" => format!(
			"OffsetDateTime::from_unix_timestamp(
				val.parse().map_err(|e| ParseError::ParseInt {{
					arg: \"{}\",
					value: val.to_string(),
					error: e,
				}})?)",
			field.pretty
		),
		_ => panic!("Unknown type '{}'", rust_type),
	};
	if res.contains('\n') { indent(&res, 2) } else { res }
}

pub fn vector_value_deserializer(field: &Field) -> String {
	let rust_type = field.get_rust_type("", true);
	let inner_type = &rust_type[4..rust_type.len() - 1];
	String::from(format!(
		"val.split(',')
						.filter_map(|val| {{
							let val = val.trim();
							if val.is_empty() {{
								None
							}} else {{
								Some(val)
							}}
						}}).map(|val| {{
							let val = val.trim();
							Ok({})
						}}).collect::<Result<Vec<{}>>>()?",
		single_value_deserializer(field, inner_type),
		inner_type
	))
}

pub fn generate_serializer(field: &Field, name: &str) -> String {
	let rust_type = field.get_rust_type("", true).replace("UidRef", "Uid");
	if rust_type.starts_with("Vec<") {
		let inner_type = &rust_type[4..rust_type.len() - 1];
		vector_value_serializer(field, inner_type, name)
	} else {
		single_value_serializer(field, &rust_type, name)
	}
}

pub fn single_value_serializer(
	field: &Field,
	rust_type: &str,
	name: &str,
) -> String
{
	match rust_type {
		"i8" | "u8" | "i16" | "u16" | "i32" | "u32" | "i64" | "u64" | "f32"
		| "f64" => format!("Cow::Owned({}.to_string())", name),
		"bool" => {
			format!("Cow::Borrowed(if {} {{ \"1\" }} else {{ \"0\" }})", name)
		}
		"&str" => format!("Cow::Borrowed({})", name),
		"String" => format!("Cow::Borrowed(&{})", name),
		"UidRef" => format!("Cow::Owned(base64::encode({}.0))", name),
		"Uid" => format!("Cow::Owned(base64::encode(&{}.0))", name),
		"ClientId" | "ClientDbId" | "ChannelId" | "ServerGroupId"
		| "ChannelGroupId" | "IconHash" => {
			format!("Cow::Owned({}.0.to_string())", name)
		}
		"TextMessageTargetMode"
		| "HostMessageMode"
		| "HostBannerMode"
		| "LicenseType"
		| "LogLevel"
		| "Codec"
		| "CodecEncryptionMode"
		| "Reason"
		| "ClientType"
		| "GroupNamingMode"
		| "GroupType"
		| "Permission"
		| "PermissionType"
		| "TokenType"
		| "PluginTargetMode"
		| "Error" => format!("Cow::Owned({}.to_u32().unwrap().to_string())", name),
		"Duration" => {
			if field.type_s == "DurationSeconds" {
				format!("Cow::Owned({}.whole_seconds().to_string())", name)
			} else if field.type_s == "DurationMilliseconds" {
				format!("Cow::Owned({}.whole_milliseconds().to_string())", name)
			} else {
				panic!("Unknown original time type {} found.", field.type_s);
			}
		}
		"OffsetDateTime" => {
			format!("Cow::Owned({}.timestamp().to_string())", name)
		}
		"IpAddr" | "SocketAddr" => format!("Cow::Owned({}.to_string())", name),
		_ => panic!("Unknown type '{}'", rust_type),
	}
}

pub fn vector_value_serializer(
	field: &Field,
	inner_type: &str,
	name: &str,
) -> String
{
	format!(
		"{{ let mut s = String::new();
				for val in {} {{
					if !s.is_empty() {{ s += \",\" }}
					let t: Cow<str> = {}; s += t.as_ref();
				}}
				Cow::Owned(s) }}",
		name,
		single_value_serializer(field, inner_type, "val")
	)
}
