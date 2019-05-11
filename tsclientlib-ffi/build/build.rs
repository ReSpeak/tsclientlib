use std::env;
use std::fs::File;
use std::io::prelude::*;
use std::path::Path;

use tsproto_structs::book::Property;

mod book_ffi;
mod cs_events;
mod events;
mod ffigen;

use crate::book_ffi::BookFfi;
use crate::events::Events;

fn main() {
	let out_dir = env::var("OUT_DIR").unwrap();

	// Write declarations
	let path = Path::new(&out_dir);
	let mut structs = File::create(&path.join("book_ffi.rs")).unwrap();
	write!(&mut structs, "{}", BookFfi::default()).unwrap();

	// Write events
	let path = Path::new(&out_dir);
	let mut structs = File::create(&path.join("events.rs")).unwrap();
	write!(&mut structs, "{}", Events::default()).unwrap();

	ffigen::gen_events().unwrap();
}

fn get_ffi_type(s: &str) -> String {
	if s.ends_with('?') {
		let inner = &s[..s.len() - 1];
		return format!("Option<{}>", get_ffi_type(inner));
	}
	match s {
		"str" => "*mut c_char",
		"ClientId" => "u16",
		"Uid" => "*mut c_char",
		"ClientDbId" => "u64",
		"ChannelId" => "u64",
		"ServerGroupId" => "u64",
		"ChannelGroupId" => "u64",
		"SocketAddr" => "*mut c_char",
		"MaxClients" => "FfiMaxClients",
		"TalkPowerRequest" => "FfiTalkPowerRequest",
		"IconHash" => "u32",
		"DateTime" => "u64",
		"Duration" => "u64",

		// Enum
		"GroupType"
		| "GroupNamingMode"
		| "Codec"
		| "ChannelType"
		| "ClientType"
		| "HostMessageMode"
		| "CodecEncryptionMode"
		| "HostBannerMode"
		| "LicenseType"
		| "TextMessageTargetMode" => "u32",
		_ => s,
	}
	.into()
}

/// Convert to ffi type
fn convert_val(type_s: &str) -> String {
	match type_s {
		"str" => "val.ffi()".into(),
		"Uid" => "val.0.ffi()".into(),
		"ClientId" | "ClientDbId" | "ChannelId" | "ServerGroupId"
		| "ChannelGroupId" | "IconHash" => "val.0".into(),
		// TODO With higher resulution than seconds?
		"DateTime" => "val.timestamp() as u64".into(),
		// TODO With higher resulution than seconds?
		"Duration" => "val.num_seconds() as u64".into(),
		"SocketAddr" => "val.to_string().ffi()".into(),
		"MaxClients" => "val.ffi()".into(),
		"TalkPowerRequest" => "val.ffi()".into(),
		// Enum
		"GroupType"
		| "GroupNamingMode"
		| "Codec"
		| "ChannelType"
		| "ClientType"
		| "HostMessageMode"
		| "CodecEncryptionMode"
		| "HostBannerMode"
		| "LicenseType"
		| "TextMessageTargetMode" => "val.to_u32().unwrap()".into(),
		_ => "*val".into(),
	}
}

/// Convert to ffi type
fn convert_property(p: &Property) -> String {
	let res = convert_val(&p.type_s);
	if p.opt {
		let default = if get_ffi_type(&p.type_s).starts_with('*') {
			"std::ptr::null_mut"
		} else {
			"Default::default"
		};
		format!(
			"val.as_ref().map(|val| {}).unwrap_or_else({})",
			res, default
		)
	} else {
		res
	}
}
