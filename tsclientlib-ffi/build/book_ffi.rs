use std::default::Default;
use tsproto_util::to_snake_case;
use tsproto_structs::*;
use tsproto_structs::book::{BookDeclarations, Struct};

#[derive(Template)]
#[TemplatePath = "build/BookFfi.tt"]
#[derive(Debug)]
pub struct BookFfi<'a>(pub &'a BookDeclarations);

impl Default for BookFfi<'static> {
	fn default() -> Self {
		BookFfi(&tsproto_structs::book::DATA)
	}
}

/// If the type is a more complex struct which cannot be returned easily.
fn is_special_type(s: &str) -> bool {
	match s {
		"SocketAddr" | "MaxClients" | "TalkPowerRequest" => true,
		_ => false,
	}
}

fn get_ffi_type(s: &str) -> String {
	match s {
		"str" => "*mut c_char",
		"ClientId" => "u16",
		"Uid" => "*mut c_char",
		"ClientDbId" => "u64",
		"ChannelId" => "u64",
		"ServerGroupId" => "u64",
		"ChannelGroupId" => "u64",
		"IconHash" => "u32",
		"DateTime" => "u64",
		"Duration" => "u64",

		// Enum
		"GroupType" | "GroupNamingMode" | "Codec" | "ChannelType" | "ClientType"
		| "HostMessageMode" | "CodecEncryptionMode" | "HostBannerMode"
		| "LicenseType" | "TextMessageTargetMode" => "u32",
		_ => s,
	}.into()
}

fn get_id_args(structs: &[Struct], struc: &Struct) -> String {
	let mut res = String::new();
	for id in &struc.id {
		let p = id.find_property(structs);
		if !res.is_empty() {
			res.push_str(", ");
		}
		res.push_str(&to_snake_case(&p.name));
		res.push_str(": ");
		if is_ref_type(&p.type_s) && p.type_s != "str" {
			res.push('&');
		}
		res.push_str(&get_ffi_type(&p.type_s).replace("mut", "const"));
	}
	res
}

fn get_id_arg_names(structs: &[Struct], struc: &Struct) -> String {
	let mut res = String::new();
	for id in &struc.id {
		let p = id.find_property(structs);
		if !res.is_empty() {
			res.push_str(", ");
		}
		res.push_str(&to_snake_case(&p.name));
	}
	res
}

/// Convert to ffi type
fn convert_val(type_s: &str, opt: bool,) -> String {
	match type_s {
		"str" => "CString::new(val.as_bytes()).unwrap().into_raw()".into(),
		"Uid" => "CString::new(val.0.as_bytes()).unwrap().into_raw()".into(),
		"ClientId" | "ClientDbId" | "ChannelId" | "ServerGroupId"
		| "ChannelGroupId" | "IconHash" => "val.0".into(),
		// TODO With higher resulution than seconds?
		"DateTime" => "val.timestamp() as u64".into(),
		// TODO With higher resulution than seconds?
		"Duration" => "val.num_seconds() as u64".into(),
		// Enum
		"GroupType" | "GroupNamingMode" | "Codec" | "ChannelType" | "ClientType"
		| "HostMessageMode" | "CodecEncryptionMode" | "HostBannerMode"
		| "LicenseType" | "TextMessageTargetMode" =>
		"val.to_u32().unwrap()".into(),
		_ => if opt {
			"**val".into()
		} else {
			"*val".into()
		}
	}
}
