use std::fmt;

use heck::*;
use lazy_static::lazy_static;
use t4rust_derive::Template;

lazy_static! {
	static ref ARRAY_KEY: RustType = RustType {
		name: String::new(),
		content: TypeContent::Builtin(BuiltinType::Primitive(PrimitiveType::Int(false, None))),
	};
}

#[derive(Template)]
#[TemplatePath = "src/Gen.tt"]
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct RustType {
	/// This has no meaning for builtin types.
	pub name: String,
	pub content: TypeContent,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum TypeContent {
	Struct(Struct),
	Enum(Enum),
	Builtin(BuiltinType),
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Struct {
	// We cannot use a map because we need the ordering
	pub fields: Vec<(String, RustType)>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Enum {
	pub possibilities: Vec<(String, Struct)>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum BuiltinType {
	/// Empty tuple `()`
	Nothing,
	Primitive(PrimitiveType),
	String,
	Str,
	Option(Box<RustType>),
	/// `Vec` or slice
	Array(Box<RustType>),
	/// `HashMap` or `BTreeMap`
	Map(Box<RustType>, Box<RustType>),
	/// `HashSet` or `BTreeSet`
	Set(Box<RustType>),
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum PrimitiveType {
	Bool,
	Char,
	/// `true` means signed, size is 8, 16, 32, 64, 128, None = isize/usize.
	Int(bool, Option<u8>),
}

impl From<PrimitiveType> for RustType {
	fn from(t: PrimitiveType) -> Self {
		BuiltinType::Primitive(t).into()
	}
}

impl From<BuiltinType> for RustType {
	fn from(t: BuiltinType) -> Self {
		Self {
			name: String::new(),
			content: TypeContent::Builtin(t),
		}
	}
}

// Try to find the right type
impl<T: AsRef<str>> From<T> for RustType {
	fn from(name: T) -> Self {
		let name = name.as_ref().chars().filter(|c| !c.is_whitespace()).collect::<String>();
		let first_char = name.chars().next();
		let (name, content) = match name.as_str() {
			// Special types
			"ConnectionId" | "FutureHandle" => return "u64".into(),
			"MessageTarget" => return "u8".into(),

			// Standard types
			"()" => (String::new(), TypeContent::Builtin(BuiltinType::Nothing)),
			"bool" => (String::new(), TypeContent::Builtin(BuiltinType::Primitive(PrimitiveType::Bool))),
			"char" => (String::new(), TypeContent::Builtin(BuiltinType::Primitive(PrimitiveType::Char))),
			"String" => (String::new(), TypeContent::Builtin(BuiltinType::String)),
			"str" | "&str" => (String::new(), TypeContent::Builtin(BuiltinType::Str)),
			n if n.len() >= 2
				&& (first_char.unwrap() == 'u' || first_char.unwrap() == 'i')
				&& n[1..].parse::<u8>().is_ok() => {
				(String::new(), TypeContent::Builtin(BuiltinType::Primitive(
					PrimitiveType::Int(n.chars().next().unwrap() == 'i', n[1..].parse::<u8>().ok()))))
			}
			n if n.starts_with("Option<") => {
				let inner = &n[n.find('<').unwrap() + 1..n.len() - 1];
				(String::new(), TypeContent::Builtin(BuiltinType::Option(Box::new(inner.to_string().into()))))
			}
			n if n.starts_with("HashSet<") | n.starts_with("BTreeSet<") => {
				let inner = &n[n.find('<').unwrap() + 1..n.len() - 1];
				(String::new(), TypeContent::Builtin(BuiltinType::Set(Box::new(inner.to_string().into()))))
			}
			n if n.starts_with("Vec<") => {
				let inner = &n[n.find('<').unwrap() + 1..n.len() - 1];
				(String::new(), TypeContent::Builtin(BuiltinType::Array(Box::new(inner.to_string().into()))))
			}
			n if n.starts_with('[') => {
				let end = n.find(';').unwrap_or(n.len() - 1);
				let inner = &n[1..end];
				(String::new(), TypeContent::Builtin(BuiltinType::Array(Box::new(inner.to_string().into()))))
			}
			n if n.starts_with("HashMap<") | n.starts_with("BTreeMap<") => {
				let inner = &n[n.find('<').unwrap() + 1..n.len() - 1];
				let mut parts = inner.split(',');
				let key = parts.next().unwrap();
				let content = parts.next().unwrap();
				(String::new(), TypeContent::Builtin(BuiltinType::Map(
					Box::new(key.to_string().into()), Box::new(content.to_string().into()))))
			}
			_ => (name, TypeContent::Struct(Struct { fields: Vec::new() })),
		};

		Self {
			name,
			content,
		}
	}
}

// Useful for creating enums with a single content field
impl From<RustType> for Struct {
	fn from(t: RustType) -> Self {
		Self { fields: vec![(String::new(), t)] }
	}
}

impl RustType {
	/// If this is a container type (of an array or a map), return the contained
	/// type.
	///
	/// Returns (key type, contained type)
	pub fn container_of(&self) -> Option<(&RustType, &RustType)> {
		match &self.content {
			TypeContent::Builtin(BuiltinType::Array(t)) => Some((&ARRAY_KEY, t)),
			TypeContent::Builtin(BuiltinType::Map(k, t)) => Some((k, t)),
			TypeContent::Builtin(BuiltinType::Set(t)) => Some((t, t)),
			_ => None,
		}
	}

	pub fn is_container(&self) -> bool {
		match &self.content {
			TypeContent::Builtin(BuiltinType::Option(_))
			| TypeContent::Builtin(BuiltinType::Array(_))
			| TypeContent::Builtin(BuiltinType::Map(_, _))
			| TypeContent::Builtin(BuiltinType::Set(_)) => true,
			_ => false,
		}
	}

	pub fn val_as_u64(&self) -> String {
		if !self.name.is_empty() {
			return format!("val as *const {} as u64", self.name);
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
					"unsafe { mem::transmute::<i64, u64>(i64::from(*val))".into()
				} else {
					"u64::from(*val)".into()
				}
			}
			BuiltinType::String | BuiltinType::Str => "val.ffi() as u64".into(),
			BuiltinType::Option(c) => format!("val.as_ref().map(|val| {})
				.unwrap_or_else(|| {{ unsafe {{ (*result).typ = FfiResultType::None; }} 0 }})",
				c.val_as_u64()),
			// TODO Get map returns array of keys, get array all values
			// TODO Support sets
			BuiltinType::Array(_) | BuiltinType::Map(_, _) => {
				panic!("Arrays and maps cannot be directly converted");
			}
			BuiltinType::Set(_) => unimplemented!(), // TODO
		}
	}

	pub fn val_from_u64(&self) -> String {
		if !self.name.is_empty() {
			return format!("unsafe {{ &*(val as *const {}) }}", self.name);
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
			}
			BuiltinType::String | BuiltinType::Str => "match ffi_to_str(val as *const c_char) {
	Ok(r) => r,
	Err(_) => return \"Failed to read string\".ffi(),
}".into(),
			BuiltinType::Option(_) => unimplemented!(), // TODO
			BuiltinType::Array(_) | BuiltinType::Map(_, _) | BuiltinType::Set(_) => {
				panic!("Arrays and maps cannot be converted");
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn simple_struct_id() {
		let t = RustType {
			name: "MyStruct".into(),
			content: TypeContent::Struct(Struct {
				fields: vec![
					("field_number_1".into(), PrimitiveType::Int(false, Some(32)).into()),
					("array".into(), BuiltinType::Array(Box::new(BuiltinType::String.into())).into()),
				],
			}),
		};
		let res = format!("{}", t);
		let split_pos = res.find("\n\n").unwrap();
		let res = &res[..split_pos];
		let res2 = "
#[derive(FromPrimitive, ToPrimitive)]
#[repr(u32)]
pub enum MyStructPropertyId {
	FieldNumber1,
	ArrayLen,
	Array,
}";
		if res != res2 {
			println!("Expected result:{}", res2);
			println!("Actual result:{}", res);
		}
		assert_eq!(res, res2);
	}

	#[test]
	fn convert_u8() {
		let real: RustType = PrimitiveType::Int(false, Some(8)).into();
		assert_eq!(real, "u8".into());
	}
}
