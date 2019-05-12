use std::fmt;
use std::ops::Deref;

use heck::*;
use t4rust_derive::Template;

use crate::*;

#[derive(Template)]
#[TemplatePath = "src/Rust.tt"]
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct RustGen(pub RustType);

impl Deref for RustGen {
	type Target = RustType;
	fn deref(&self) -> &Self::Target { &self.0 }
}

trait RustTypeRustExt {
	fn val_to_u64(&self) -> String;
	fn val_from_u64(&self) -> String;
}

impl RustTypeRustExt for RustType {
	fn val_to_u64(&self) -> String {
		if !self.name.is_empty() {
			return format!("val as *const {} as u64", self.name);
		}
		if let Some(w) = &self.wrapper {
			if let Some(fun) = &w.to_u64 {
				return fun.clone();
			} else {
				return format!("val.into()");
			}
		}

		let t = if let TypeContent::Builtin(t) = &self.content {
			t
		} else {
			panic!("Structs and enums always must have names")
		};
		match t {
			BuiltinType::Nothing => "0".into(),
			BuiltinType::Primitive(p) => match p {
				PrimitiveType::Bool | PrimitiveType::Char =>
					"*val as u64".into(),
				PrimitiveType::Int(s, _) => if *s {
					"unsafe { std::mem::transmute::<i64, u64>(i64::from(*val)) }".into()
				} else {
					"u64::from(*val)".into()
				}
				PrimitiveType::Float(64) =>
					"unsafe { std::mem::transmute::<f64, u64>(val) }".into(),
				PrimitiveType::Float(_) =>
					"unsafe { std::mem::transmute::<f64, u64>(f64::from(*val)) }".into(),
			}
			BuiltinType::String | BuiltinType::Str => "val.ffi() as u64".into(),
			BuiltinType::Option(c) => format!("val.as_ref().map(|val| {})
				.unwrap_or_else(|| {{ unsafe {{ (*result).typ = FfiResultType::None; }} 0 }})",
				c.val_to_u64()),
			BuiltinType::Array(inner) => {
				// Get all items
				format!("
			Box::into_raw(val.iter().map(|val| {})
				.collect::<Vec<_>>().into_boxed_slice())",
					inner.val_to_u64())
			}
			BuiltinType::Map(key, _) | BuiltinType::Set(key) => {
				let iter = if let BuiltinType::Map(_, _) = t {
					"keys"
				} else {
					"iter"
				};

				// Get all keys
				format!("
			Box::into_raw(val.{}().map(|val| {})
				.collect::<Vec<_>>().into_boxed_slice())",
					iter, key.val_to_u64())
			}
		}
	}

	fn val_from_u64(&self) -> String {
		if !self.name.is_empty() {
			return format!("unsafe {{ &*(val as *const {}) }}", self.name);
		}
		if let Some(w) = &self.wrapper {
			if let Some(fun) = &w.from_u64 {
				return fun.clone();
			} else {
				return format!("val.into()");
			}
		}

		let t = if let TypeContent::Builtin(t) = &self.content {
			t
		} else {
			panic!("Structs and enums always must have names")
		};
		match t {
			BuiltinType::Nothing => "()".into(),
			BuiltinType::Primitive(p) => match p {
				PrimitiveType::Bool => "val as bool".into(),
				PrimitiveType::Char => "val as char".into(),
				PrimitiveType::Int(s, None) => format!("val as {}size",
					if *s { "i" } else { "u" }),
				PrimitiveType::Int(s, Some(si)) => format!("val as {}{}",
					if *s { "i" } else { "u" }, si),
				PrimitiveType::Float(64) => "unsafe { std::mem::transmute<u64, f64>(val) }".into(),
				PrimitiveType::Float(i) => format!("unsafe {{ std::mem::transmute<u64, f64>(val) }} as f{}", i),
			}
			BuiltinType::String | BuiltinType::Str => r#"match ffi_to_str(val as *const c_char) {
	Ok(r) => r,
	Err(_) => {
		unsafe {
			(*result).content = "Failed to read string".ffi() as u64;
			(*result).typ = FfiResultType::Error;
		}
		return;
	}
}"#.into(),
			BuiltinType::Option(_)
			| BuiltinType::Array(_)
			| BuiltinType::Map(_, _)
			| BuiltinType::Set(_) => {
				panic!("Arrays and maps cannot be converted");
			}
		}
	}
}
