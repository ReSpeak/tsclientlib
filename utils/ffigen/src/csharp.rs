use std::fmt;
use std::ops::Deref;

use heck::*;
use t4rust_derive::Template;

use crate::*;

#[derive(Template)]
#[TemplatePath = "src/CSharp.tt"]
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct CSharpGen(pub RustType);

impl Deref for CSharpGen {
	type Target = RustType;
	fn deref(&self) -> &Self::Target { &self.0 }
}

trait RustTypeCSharpExt {
	fn val_to_u64(&self) -> String;
	fn val_from_u64(&self) -> String;
}

trait PrimitiveTypeCSharpExt {
	fn get_name(&self) -> &'static str;
}

impl RustTypeCSharpExt for RustType {
	fn val_to_u64(&self) -> String {
		if !self.name.is_empty() {
			return "val".into();
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
				PrimitiveType::Bool | PrimitiveType::Char | PrimitiveType::Int(_, _) =>
					"(ulong)val".into(),
				PrimitiveType::Float(s) =>
					format!("new FloatConverter {{ F{}Value = val }}.U64Value", s),
			}
			// TODO This needs using (Utf8String)
			BuiltinType::String | BuiltinType::Str => "val.ffi() as u64".into(),
			BuiltinType::Option(c) => c.val_to_u64(),
			BuiltinType::Array(_) | BuiltinType::Map(_, _) | BuiltinType::Set(_) => {
				panic!("Arrays and maps cannot be directly converted");
			}
		}
	}

	fn val_from_u64(&self) -> String {
		if !self.name.is_empty() {
			return "val".into();
		}
		if let Some(w) = &self.wrapper {
			if let Some(fun) = &w.from_u64 {
				return fun.clone();
			} else {
				return format!("val.to_ulong()");
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
				PrimitiveType::Bool => "val != 0".into(),
				PrimitiveType::Char => "(char)val".into(),
				PrimitiveType::Int(_, _) => format!("({})val", p.get_name()),
				PrimitiveType::Float(s) =>
					format!("new FloatConverter {{ U64Value = val }}.F{}Value", s),
			}
			BuiltinType::String | BuiltinType::Str => "NativeMethods.StringFromNativeUtf8((IntPtr)val)".into(),
			BuiltinType::Option(inner) => inner.val_from_u64(),
			BuiltinType::Map(key, inner) => {
				let key_name = if let Some(wrapper) = &key.wrapper {
					&wrapper.outer
				} else {
					&key.name
				};
				let inner_name = &inner.name;

				// Free the items if necessary
				let free_fun = if key.is_primitive() {
					"NativeMethods.tscl_free_u64s(lis, (UIntPtr)length);".into()
				} else {
					format!("NativeMethods.tscl_free_u64_{}s(lis, (UIntPtr)length);", self.name)
				};

				let convert_keys = format!("{{ unsafe {{
		var lis = (ulong*)list;
		var res = new List<{}>((int)length);
		for (ulong i = 0; i < length; i++)
		{{
			var val = *(lis + i);
			res.Add({});
		}}
		{}
		return res;
	}} }}", key_name, key.val_from_u64(), free_fun);

				let type_s = format!("FakeDictionary<{}, {}>", key_name, inner_name);
				format!("\
List<ulong> id = new List<ulong>(this.id);
id.Add(idVal);
return new {}(basePtr, baseFunction, id)
{{
	containsObjects = true,
	convertKey = val => {},
	convertKeyArray = (list, length) => {},
	createValue = (basePtr, baseFunction, i) =>
		new {}(basePtr, baseFunction, i),
}};", type_s, key.val_to_u64(), convert_keys, inner_name)
			}
			BuiltinType::Array(_) | BuiltinType::Set(_) => {
				panic!("Arrays and maps cannot be converted");
			}
		}
	}
}

impl PrimitiveTypeCSharpExt for PrimitiveType {
	fn get_name(&self) -> &'static str {
		match self {
			PrimitiveType::Bool => "bool",
			PrimitiveType::Char => "char",
			PrimitiveType::Int(false, Some(8)) => "byte",
			PrimitiveType::Int(false, Some(16)) => "ushort",
			PrimitiveType::Int(false, Some(32)) => "uint",
			PrimitiveType::Int(false, Some(64)) => "ulong",
			PrimitiveType::Int(true, Some(8)) => "sbyte",
			PrimitiveType::Int(true, Some(16)) => "short",
			PrimitiveType::Int(true, Some(32)) => "int",
			PrimitiveType::Int(true, Some(64)) => "long",
			PrimitiveType::Int(false, None) => "UIntPtr",
			PrimitiveType::Int(true, None) => "IntPtr",
			PrimitiveType::Float(32) => "float",
			PrimitiveType::Float(64) => "double",
			_ => panic!("Unknown primitive type"),
		}
	}
}
