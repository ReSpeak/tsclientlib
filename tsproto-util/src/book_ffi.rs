use tsproto_structs::book::{BookDeclarations, Struct};
use *;

#[derive(Template)]
#[TemplatePath = "src/BookFfi.tt"]
#[derive(Debug)]
pub struct BookFfi<'a>(pub &'a BookDeclarations);

/// If the type is a more complex struct which cannot be returned easily.
fn is_special_type(s: &str) -> bool {
	match s {
		"SocketAddr" | "MaxFamilyClients" | "TalkPowerRequest" => true,
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
