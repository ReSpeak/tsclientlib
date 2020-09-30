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
		res
	}

	pub fn c2s() -> messages::MessageDeclarations {
		let mut res = DATA.clone();
		res.msg_group.retain(|g| g.default.c2s);
		res
	}
}

impl Default for MessageDeclarations<'static> {
	fn default() -> Self { MessageDeclarations(&DATA) }
}

pub fn generate_deserializer(field: &Field) -> String {
	let rust_type = field.get_rust_type("", false);
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
				source: e,
			}})?",
			field.pretty
		),
		"f32" | "f64" => format!(
			"val.parse().map_err(|e| ParseError::ParseFloat {{
				arg: \"{}\",
				value: val.to_string(),
				source: e,
			}})?",
			field.pretty
		),
		"bool" => format!(
			"match val {{ \"0\" => false, \"1\" => true, _ => return Err(ParseError::ParseBool {{
				arg: \"{}\",
				value: val.to_string(),
			}}), }}",
			field.pretty
		),
		"Uid" => "Uid(if let Ok(uid) = base64::decode(val) { uid } else { val.as_bytes().to_vec() \
		          })"
		.into(),
		"&str" => "val".into(),
		"String" => "val.to_string()".into(),
		"IconId" => format!(
			"IconId(if val.starts_with('-') {{
			val.parse::<i32>().map(|i| i as u32)
		}} else {{
			val.parse::<u64>().map(|i| i as u32)
		}}.map_err(|e| ParseError::ParseInt {{
			arg: \"{}\",
			value: val.to_string(),
			source: e,
		}})?)",
			field.pretty
		),
		"ClientId" | "ClientDbId" | "ChannelId" | "ServerGroupId" | "ChannelGroupId" => format!(
			"{}(val.parse().map_err(|e| ParseError::ParseInt {{
				arg: \"{}\",
				value: val.to_string(),
				source: e,
			}})?)",
			rust_type, field.pretty
		),
		"IpAddr" | "SocketAddr" => format!(
			"val.parse().map_err(|e| ParseError::ParseAddr {{
				arg: \"{}\",
				value: val.to_string(),
				source: e,
			}})?",
			field.pretty
		),
		"ClientType" => format!(
			"match val {{
				\"0\" => ClientType::Normal,
				\"1\" => ClientType::Query {{ admin: false }},
				_ => return Err(ParseError::InvalidValue {{
					arg: \"{}\",
					value: val.to_string(),
				}}),
			}}",
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
				source: e,
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
				source: e,
			}})?).ok_or(ParseError::InvalidValue {{
				arg: \"{1}\",
				value: val.to_string(),
				}})?",
			rust_type, field.pretty
		),
		"Duration" => {
			if field.type_s == "DurationSeconds" {
				format!(
					"let val = val.parse::<i64>().map_err(|e| ParseError::ParseInt {{
					arg: \"{}\",
					value: val.to_string(),
					source: e,
				}})?;
				if val.checked_mul(1000).is_some() {{ Duration::seconds(val) }}
				else {{ return Err(ParseError::InvalidValue {{
					arg: \"{0}\",
					value: val.to_string(),
					}}); }}",
					field.pretty
				)
			} else if field.type_s == "DurationMilliseconds" {
				format!(
					"Duration::milliseconds(val.parse::<i64>().map_err(|e| ParseError::ParseInt {{
					arg: \"{}\",
					value: val.to_string(),
					source: e,
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
					source: e,
				}})?)",
			field.pretty
		),
		_ => panic!("Unknown type '{}' when trying to deserialize {:?}", rust_type, field),
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

pub fn generate_serializer(field: &Field, name: &str, is_ref: bool) -> String {
	let rust_type = field.get_rust_type("", false);
	if rust_type.starts_with("Vec<") {
		let inner_type = &rust_type[4..rust_type.len() - 1];
		vector_value_serializer(field, inner_type, name, is_ref)
	} else {
		single_value_serializer(field, &rust_type, name, is_ref)
	}
}

pub fn single_value_serializer(field: &Field, rust_type: &str, name: &str, is_ref: bool) -> String {
	let ref_amp = if is_ref { "" } else { "&" };
	let ref_star = if is_ref { "*" } else { "" };
	match rust_type {
		"i8" | "u8" | "i16" | "u16" | "i32" | "u32" | "i64" | "u64" | "f32" | "f64" | "String"
		| "IpAddr" | "SocketAddr" => format!("{}{}", ref_amp, name),
		"bool" => format!("if {}{} {{ &\"1\" }} else {{ &\"0\" }}", ref_star, name),
		"&str" => name.to_string(),
		"UidRef" | "Uid" => format!(
			"&if {1}.0 == b\"ServerAdmin\" {{ Cow::Borrowed(\"ServerAdmin\") }}
			else {{ Cow::<str>::Owned(base64::encode({0}{1}.0)) }}",
			if rust_type == "Uid" { "&" } else { "" },
			name,
		),
		"ClientId" | "ClientDbId" | "ChannelId" | "ServerGroupId" | "ChannelGroupId"
		| "IconId" => format!("&{}.0", name),
		"ClientType" => format!(
			"match {} {{
				ClientType::Normal => &\"0\",
				ClientType::Query {{ .. }} => &\"1\",
			}}",
			name
		),
		"TextMessageTargetMode"
		| "HostMessageMode"
		| "HostBannerMode"
		| "LicenseType"
		| "LogLevel"
		| "Codec"
		| "CodecEncryptionMode"
		| "Reason"
		| "GroupNamingMode"
		| "GroupType"
		| "Permission"
		| "PermissionType"
		| "TokenType"
		| "PluginTargetMode"
		| "Error" => format!("&{}.to_u32().unwrap()", name),
		"ChannelPermissionHint" | "ClientPermissionHint" => format!("&{}.bits()", name),
		"Duration" => {
			if field.type_s == "DurationSeconds" {
				format!("&{}.whole_seconds()", name)
			} else if field.type_s == "DurationMilliseconds" {
				format!("&{}.whole_milliseconds()", name)
			} else {
				panic!("Unknown original time type {} found.", field.type_s);
			}
		}
		"OffsetDateTime" => format!("&{}.timestamp()", name),
		_ => panic!("Unknown type '{}'", rust_type),
	}
}

pub fn vector_value_serializer(
	field: &Field, inner_type: &str, name: &str, is_ref: bool,
) -> String {
	// TODO Vector serialization creates an intermediate string which is not necessary
	format!(
		"&{{ let mut s = String::new();
				for val in {}{} {{
					if !s.is_empty() {{ s += \",\" }}
					write!(&mut s, \"{{}}\", {}).unwrap();
				}}
				s
			}}",
		if is_ref { "" } else { "&" },
		name,
		single_value_serializer(field, inner_type, "val", true)
	)
}
