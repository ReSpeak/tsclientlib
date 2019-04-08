use std::env;
use std::fs::File;
use std::io::prelude::*;
use std::path::Path;

mod book_ffi;
mod events;

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
		"GroupType" | "GroupNamingMode" | "Codec" | "ChannelType" | "ClientType"
		| "HostMessageMode" | "CodecEncryptionMode" | "HostBannerMode"
		| "LicenseType" | "TextMessageTargetMode" => "u32",
		_ => s,
	}.into()
}
