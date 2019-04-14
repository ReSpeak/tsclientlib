use std::default::Default;

use t4rust_derive::Template;
use tsproto_structs::*;
use tsproto_structs::book::{BookDeclarations, Struct};
use tsproto_structs::book_to_messages::{BookToMessagesDeclarations, Event,
	RuleKind, RuleOp};

use crate::*;

#[derive(Template)]
#[TemplatePath = "build/BookFfi.tt"]
#[derive(Debug)]
pub struct BookFfi<'a>(pub &'a BookDeclarations, pub &'a BookToMessagesDeclarations<'a>);

impl Default for BookFfi<'static> {
	fn default() -> Self {
		BookFfi(&tsproto_structs::book::DATA, &tsproto_structs::book_to_messages::DATA)
	}
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

/// Convert ffi type to rust type
fn convert_to_rust(name: &str, type_s: &str) -> String {
	if type_s.ends_with('?') {
		let inner = &type_s[..type_s.len() - 1];
		return format!("{}.map(|v| {})", name, convert_to_rust("v", inner));
	}
	match type_s {
		"str" => format!("unsafe {{ CStr::from_ptr({}).to_str().unwrap() }}", name),
		"Uid" => format!("unsafe {{ UidRef(CStr::from_ptr({}).to_str().unwrap()) }}", name),
		"ClientId" | "ClientDbId" | "ChannelId" | "ServerGroupId"
		| "ChannelGroupId" | "IconHash" => format!("{}({})", type_s, name),
		"DateTime" => format!("DateTime::from_utc(NaiveDateTime::from_timestamp({}, 0), Utc)", name),
		"Duration" => format!("Duration::new({}, 0)", name),
		// Enum
		"GroupType" | "GroupNamingMode" | "Codec" | "ChannelType" | "ClientType"
		| "HostMessageMode" | "CodecEncryptionMode" | "HostBannerMode"
		| "LicenseType" | "TextMessageTargetMode" =>
		format!("{}.from_u32({}).unwrap()", type_s, name),
		_ => name.into(),
	}
}

fn get_ffi_arguments_def(r: &RuleKind) -> String {
	match r {
		RuleKind::Map { .. } | RuleKind::Function { .. } =>
			format!("{}: {}", to_snake_case(r.from_name()),
				get_ffi_type(&r.from().type_s).replace("mut", "const")),
		RuleKind::ArgumentMap { from, .. } =>
			format!("{}: {}", to_snake_case(from),
				get_ffi_type(&r.from().type_s).replace("mut", "const")),
		RuleKind::ArgumentFunction { from, type_s, .. } =>
			format!("{}: {}", to_snake_case(from),
				get_ffi_type(type_s).replace("mut", "const")),
	}
}

fn get_ffi_arguments(r: &RuleKind) -> String {
	match r {
		RuleKind::Map { .. } | RuleKind::Function { .. } =>
			convert_to_rust(&to_snake_case(r.from_name()), &r.from().type_s),
		RuleKind::ArgumentMap { from, .. } =>
			convert_to_rust(&to_snake_case(from), &r.from().type_s),
		RuleKind::ArgumentFunction { from, type_s, .. } =>
			convert_to_rust(&to_snake_case(from), type_s),
	}
}

fn get_all_arguments_def<'a>(e: &'a Event<'a>, r: Option<&'a RuleKind<'a>>) -> String {
	let mut args = String::new();
	for r in e.ids.iter().chain(r.iter().cloned()) {
		match r {
			RuleKind::ArgumentMap { .. } |
			RuleKind::ArgumentFunction { .. } => {
				let arg = get_ffi_arguments_def(r);
					args.push_str(&arg);
					args.push_str(", ");
			}
			_ => {}
		}
	}
	args
}

fn get_all_arguments<'a>(e: &'a Event<'a>, r: Option<&'a RuleKind<'a>>) -> String {
	let mut args = String::new();
	for r in e.ids.iter().chain(r.iter().cloned()) {
		match r {
			RuleKind::ArgumentMap { .. } |
			RuleKind::ArgumentFunction { .. } => {
				let arg = get_ffi_arguments(r);
					args.push_str(&arg);
					args.push_str(", ");
			}
			_ => {}
		}
	}
	args
}
