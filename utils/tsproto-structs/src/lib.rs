//! `tsproto-structs` contains machine readable data for several TeamSpeak
//! related topics.
//!
//! The underlying data files can be found in the [tsdeclarations](https://github.com/ReSpeak/tsdeclarations)
//! repository.
//!
//! The contained data may change with any version so the suggested way of
//! referring to this crate is using `tsproto-structs = "=0.1.0"`.
//!
//! The helper functions found in the root of this crate may also change with
//! any version.
//!
//! A change in the way data are stored and made accessible is considered an API
//! breaking change and will increment the minor version number.

use serde::Deserialize;

pub mod book;
pub mod book_to_messages;
pub mod enums;
pub mod errors;
pub mod messages;
pub mod messages_to_book;
pub mod permissions;
pub mod versions;

#[derive(Debug, Deserialize)]
pub struct EnumValue {
	pub name: String,
	pub doc: String,
	pub num: String,
}

fn get_false() -> bool { false }

pub fn is_ref_type(s: &str) -> bool {
	if s.starts_with("Option<") {
		is_ref_type(&s[7..s.len() - 1])
	} else {
		!(s == "bool"
			|| s.starts_with('i')
			|| s.starts_with('u')
			|| s.starts_with('f')
			|| s.ends_with("Id")
			|| s.ends_with("Type")
			|| s.ends_with("Mode"))
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
	if t.ends_with('?') {
		let inner = &t[..(t.len() - 1)];
		return format!("Option<{}>", convert_type(inner, is_ref));
	}
	if t.ends_with('T') {
		return convert_type(&t[..(t.len() - 1)], is_ref);
	}

	if t == "str" || t == "string" {
		if is_ref { String::from("&str") } else { String::from("String") }
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
	} else if t == "DateTime" {
		String::from("OffsetDateTime")
	} else if t.starts_with("Duration") {
		String::from("Duration")
	} else if t == "ClientUid" {
		if is_ref { String::from("UidRef") } else { String::from("Uid") }
	} else if ["TalkPowerRequest", "EccKeyPubP256"].contains(&t) && is_ref {
		format!("&{}", t)
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

/// Prepend `/// ` to each line of a string.
pub fn doc_comment(s: &str) -> String { s.lines().map(|l| format!("/// {}\n", l)).collect() }

/// Indent a string by a given count using tabs.
pub fn indent<S: AsRef<str>>(s: S, count: usize) -> String {
	let sref = s.as_ref();
	let line_count = sref.lines().count();
	let mut result = String::with_capacity(sref.len() + line_count * count * 4);
	for l in sref.lines() {
		if !l.is_empty() {
			result.push_str(std::iter::repeat("\t").take(count).collect::<String>().as_str());
		}
		result.push_str(l);
		result.push('\n');
	}
	result
}

/// Unindent a string by a given count of tabs.
pub fn unindent(mut s: &mut String) {
	std::mem::swap(&mut s.replace("\n\t", "\n"), &mut s);
	if s.get(0..1) == Some("\t") {
		s.remove(0);
	}
}
