use std::collections::HashMap;
use std::{env, fs};
use std::path::Path;

use ffigen::{RustType, Wrapper};
use syn::*;

type Result<T> = std::result::Result<T, failure::Error>;

fn extract_items(p: &Path, names: &[&str], items: &mut Vec<Item>) -> Result<()> {
	let file = fs::read_to_string(p)?;
	let syntax = syn::parse_file(&file)?;
	for item in syntax.items {
		match &item {
			Item::Struct(s) => if names.contains(&s.ident.to_string().as_str()) {
				items.push(item);
			}
			Item::Enum(s) => if names.contains(&s.ident.to_string().as_str()) {
				items.push(item);
			}
			_ => {}
		}
	}

	Ok(())
}

pub fn gen_events() -> Result<()> {
	let base_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
	let base_dir = Path::new(&base_dir);
	let out_dir = env::var("OUT_DIR").unwrap();
	let out_dir = Path::new(&out_dir);
	// TODO Rerun if files changed

	let mut items = Vec::new();
	extract_items(
		&base_dir.join("src").join("lib.rs"),
		&["EventContent", "NewEvent", "NewFfiInvoker"],
		&mut items,
	)?;

	extract_items(
		&base_dir.join("..").join("tsclientlib").join("src").join("events.rs"),
		&["Event"],
		&mut items,
	)?;

	extract_items(
		&base_dir.join("..").join("tsclientlib").join("src").join("lib.rs"),
		&["MessageTarget"],
		&mut items,
	)?;

	extract_items(
		&base_dir.join("..").join("utils").join("tsproto-commands").join("src").join("lib.rs"),
		&["Invoker", "MaxClients", "TalkPowerRequest"],
		&mut items,
	)?;

	// TODO
	extract_items(
		&base_dir.join("..").join("target").join("debug")
			.join("build").join("tsclientlib-b7db218433c96145").join("out")
			.join("events.rs"),
		&["PropertyId", "PropertyValue"],
		&mut items,
	)?;

	// TODO
	extract_items(
		&base_dir.join("..").join("target").join("debug")
			.join("build").join("tsclientlib-b7db218433c96145").join("out")
			.join("structs.rs"),
		&["Channel", "ChatEntry", "Client",
			"Connection", "ConnectionClientData", "ConnectionServerData", "File",
			"OptionalChannelData", "OptionalClientData",
			"OptionalServerData", "Server", "ServerGroup"],
		&mut items,
	)?;

	// Generate code
	let wrappers = vec![("FutureHandle", "u64")];
	let mut wrappers: HashMap<_, _> = wrappers.into_iter()
		.map(|(a, b)| (a.into(), b.into()))
		.collect();

	// TODO Other time resolution?
	let mut typ: RustType = "u64".into();
	typ.wrapper = Some(Wrapper {
		outer: "DateTime<Utc>".into(),
		to_u64: Some("val.timestamp() as u64".into()),
		from_u64: Some("DateTime::from_utc(NaiveDateTime::from_timestamp(val, 0), Utc)".into()),
	});
	wrappers.insert("DateTime<Utc>".into(), typ);

	// TODO Other time resolution?
	let mut typ: RustType = "u64".into();
	typ.wrapper = Some(Wrapper {
		outer: "Duration".into(),
		to_u64: Some("val.num_seconds() as u64".into()),
		from_u64: Some("Duration::new(val, 0)".into()),
	});
	wrappers.insert("Duration".into(), typ);

	let mut typ: RustType = "String".into();
	typ.wrapper = Some(Wrapper {
		outer: "SocketAddr".into(),
		to_u64: Some("val.to_string().ffi() as u64".into()),
		from_u64: Some(r#"match ffi_to_str(val as *const c_char) {
	Ok(r) => match r.parse() {
		Ok(r) => r,
		Err(_) => {
			unsafe {
				(*result).content = format!("Failed to parse socket address {:?}", r).ffi() as u64;
				(*result).typ = FfiResultType::Error;
			}
			return;
		}
	},
	Err(_) => {
		unsafe {
			(*result).content = "Failed to read string".ffi() as u64;
			(*result).typ = FfiResultType::Error;
		}
		return;
	}
}"#.into()),
	});
	wrappers.insert("SocketAddr".into(), typ);

	let mut typ: RustType = "String".into();
	typ.wrapper = Some(Wrapper {
		outer: "Uid".into(),
		to_u64: Some("val.0.ffi() as u64".into()),
		from_u64: Some(r#"match ffi_to_str(val as *const c_char) {
	Ok(r) => Uid(r),
	Err(_) => {
		unsafe {
			(*result).content = "Failed to read string".ffi() as u64;
			(*result).typ = FfiResultType::Error;
		}
		return;
	}
}"#.into()),
	});
	wrappers.insert("Uid".into(), typ);

	let mut typ: RustType = "u64".into();
	typ.wrapper = Some(Wrapper {
		outer: "ClientId".into(),
		to_u64: Some("u64::from(val.0)".into()),
		from_u64: Some("ClientId(val as u16)".into()),
	});
	wrappers.insert("ClientId".into(), typ);

	for t in ["ChannelGroupId", "ChannelId", "ClientDbId", "ConnectionId",
		"IconHash", "ServerGroupId"].iter().cloned() {
		let mut typ: RustType = "u64".into();
		typ.wrapper = Some(Wrapper {
			outer: t.into(),
			to_u64: Some("u64::from(val.0)".into()),
			from_u64: Some(format!("{}(val)", t).into()),
		});
		wrappers.insert(t.into(), typ);
	}

	for t in ["ChannelType", "ClientType", "Codec", "CodecEncryptionMode",
		"GroupNamingMode", "GroupType", "HostBannerMode", "HostMessageMode",
		"LicenseType", "FfiMessageTarget", "TextMessageTargetMode"].iter().cloned() {
		let mut typ: RustType = "u64".into();
		typ.wrapper = Some(Wrapper {
			outer: t.into(),
			to_u64: Some("val.to_u64().unwrap()".into()),
			from_u64: Some(format!(r#"match {}::from_u64(*val) {{
	Some(r) => r,
	None => {{
		unsafe {{
			(*result).content = format!("Invalid {} {{}}", val).ffi() as u64;
			(*result).typ = FfiResultType::Error;
		}}
		return;
	}}
}}"#, t, t).into()),
		});
		wrappers.insert(t.into(), typ);
	}

	// TODO u64 <-> ClientId, ServerGroupId, ClientId, ChannelGroupId, GroupType, other enums

	let mut res = String::new();
	for ty in items.iter().map(|i| ffigen::convert_item(i, &wrappers)) {
		res.push_str(&ty.to_string());
	}
	fs::write(&out_dir.join("ffigen.rs"), res.as_bytes())?;


	Ok(())
}
