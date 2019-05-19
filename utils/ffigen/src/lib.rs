use std::collections::HashMap;

use heck::*;
use lazy_static::lazy_static;
use syn::*;
use syn::punctuated::Punctuated;
use syn::token::Comma;
use quote::ToTokens;

mod csharp;
mod rust;

pub use csharp::CSharpGen;
pub use rust::RustGen;

lazy_static! {
	static ref ARRAY_KEY: RustType = RustType {
		name: String::new(),
		wrapper: None,
		content: TypeContent::Builtin(BuiltinType::Primitive(PrimitiveType::Int(false, None))),
		setters: Default::default(),
		methods: Default::default(),
	};
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct RustType {
	/// This has no meaning for builtin types.
	/// For wrapped types, this is empty (like for builtin types) but the
	/// wrapper is set.
	pub name: String,
	pub wrapper: Option<Wrapper>,
	pub content: TypeContent,
	pub setters: Vec<Setter>,
	pub methods: Vec<Method>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Wrapper {
	/// The name of the type that wrapes the inner type.
	pub outer: String,
	/// The function to convert from the wrapped type to an `u64`.
	///
	/// If this is not set, `.into()` will be used.
	pub to_u64: Option<String>,
	/// The function to convert from an `u64` to the wrapped type.
	///
	/// If this is not set, `.into()` will be used.
	pub from_u64: Option<String>,
}

/// A setter always calls a method.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Setter {
	/// Type of the field which gets set.
	typ: RustType,
	/// Name of the field which gets set.
	field: String,
	/// If this contains the name of a method, this property can be directly set,
	/// else this property is a struct and its fields can be set.
	///
	/// The first part is the name of the setter, the second part is the return
	/// type.
	setter: Option<(String, RustType)>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Argument {
	/// Type of the argument
	typ: RustType,
	/// Name of the argument
	field: String,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Method {
	args: Vec<Argument>,
	ret_type: RustType,
	/// Name of the method
	name: String,
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
	Float(u8),
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
			wrapper: None,
			content: TypeContent::Builtin(t),
			setters: Default::default(),
			methods: Default::default(),
		}
	}
}

// Try to find the right type
impl<T: AsRef<str>> From<T> for RustType {
	fn from(name: T) -> Self {
		Self::from_with_wrappers(name, &HashMap::new())
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

	pub fn is_primitive(&self) -> bool {
		if let TypeContent::Builtin(BuiltinType::Primitive(_)) = self.content {
			true
		} else {
			false
		}
	}

	pub fn has_content(&self) -> bool {
		match &self.content {
			TypeContent::Struct(s) => !s.fields.is_empty(),
			TypeContent::Enum(s) => !s.possibilities.is_empty(),
			_ => false,
		}
	}

	/// For every field in the struct or enum returns
	/// `(prefix, name, type)`.
	///
	/// The prefix is empty for structs or the enum possibility name for enums.
	pub fn get_all_fields(&self) -> Vec<(&str, &str, &RustType)> {
		match &self.content {
			TypeContent::Struct(s) => {
				s.fields.iter().map(|(n, t)| ("", n.as_str(), t)).collect()
			}
			TypeContent::Enum(s) => {
				s.possibilities.iter().map(|(p, s)| {
					s.fields.iter().map(move |(n, t)| (p.as_str(), n.as_str(), t))
				}).flatten().collect()
			}
			_ => Vec::new(),
		}
	}

	/// The wrapper map the name of wrapper-types to wrapped types.
	///
	/// E.g. `struct MyId(u64)` has an entry `wrappers["MyId"] = u64`.
	pub fn from_with_wrappers<S: AsRef<str>>(name: S, wrappers: &HashMap<String, RustType>) -> Self {
		let name = name.as_ref().chars().filter(|c| !c.is_whitespace()).collect::<String>();
		if let Some(t) = wrappers.get(&name) {
			return t.clone();
		}

		let first_char = name.chars().next();
		let (name, content) = match name.as_str() {
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
			n if n.len() >= 2
				&& first_char.unwrap() == 'f'
				&& n[1..].parse::<u8>().is_ok() => {
				(String::new(), TypeContent::Builtin(BuiltinType::Primitive(
					PrimitiveType::Float(n[1..].parse::<u8>().unwrap()))))
			}
			n if n.starts_with("Option<") => {
				let inner = &n[n.find('<').unwrap() + 1..n.len() - 1];
				(String::new(), TypeContent::Builtin(BuiltinType::Option(Box::new(
					Self::from_with_wrappers(inner.to_string(), wrappers)))))
			}
			n if n.starts_with("HashSet<") | n.starts_with("BTreeSet<") => {
				let inner = &n[n.find('<').unwrap() + 1..n.len() - 1];
				(String::new(), TypeContent::Builtin(BuiltinType::Set(Box::new(
					Self::from_with_wrappers(inner.to_string(), wrappers)))))
			}
			n if n.starts_with("Vec<") => {
				let inner = &n[n.find('<').unwrap() + 1..n.len() - 1];
				(String::new(), TypeContent::Builtin(BuiltinType::Array(Box::new(
					Self::from_with_wrappers(inner.to_string(), wrappers)))))
			}
			n if n.starts_with('[') => {
				let end = n.find(';').unwrap_or(n.len() - 1);
				let inner = &n[1..end];
				(String::new(), TypeContent::Builtin(BuiltinType::Array(Box::new(
					Self::from_with_wrappers(inner.to_string(), wrappers)))))
			}
			n if n.starts_with("HashMap<") | n.starts_with("BTreeMap<") => {
				let inner = &n[n.find('<').unwrap() + 1..n.len() - 1];
				let mut parts = inner.split(',');
				let key = parts.next().unwrap();
				let content = parts.next().unwrap();
				(String::new(), TypeContent::Builtin(BuiltinType::Map(
					Box::new(Self::from_with_wrappers(key.to_string(), wrappers)),
					Box::new(Self::from_with_wrappers(content.to_string(), wrappers)))))
			}
			_ => (name, TypeContent::Struct(Struct { fields: Vec::new() })),
		};

		Self {
			name,
			wrapper: None,
			content,
			setters: Default::default(),
			methods: Default::default(),
		}
	}
}

fn fields_to_struct(fields: &Fields, wrappers: &HashMap<String, RustType>) -> Struct {
	let mut struc = Struct { fields: Vec::new() };
	match fields {
		Fields::Named(n) => for f in n.named.iter() {
			let n = f.ident.as_ref().map(|i| i.to_string()).unwrap_or_else(String::new);
			let ty = f.ty.clone().into_token_stream().to_string();
			struc.fields.push((n, RustType::from_with_wrappers(&ty, wrappers)));
		}
		Fields::Unnamed(n) => for f in n.unnamed.iter() {
			let n = f.ident.as_ref().map(|i| i.to_string()).unwrap_or_else(String::new);
			let ty = f.ty.clone().into_token_stream().to_string();
			struc.fields.push((n, RustType::from_with_wrappers(&ty, wrappers)));
		}
		Fields::Unit => {}
	}
	struc
}

pub fn convert_struct(name: &Ident, fields: &Fields, wrappers: &HashMap<String, RustType>) -> RustType {
	RustType {
		name: name.to_string(),
		wrapper: None,
		content: TypeContent::Struct(fields_to_struct(fields, wrappers)),
		setters: Default::default(),
		methods: Default::default(),
	}
}

pub fn convert_enum(name: &Ident, variants: &Punctuated<Variant, Comma>, wrappers: &HashMap<String, RustType>) -> RustType {
	let mut en = Enum { possibilities: Vec::new() };
	for v in variants {
		let prefix = v.ident.to_string();
		en.possibilities.push((prefix, fields_to_struct(&v.fields, wrappers)));
	}
	RustType {
		name: name.to_string(),
		wrapper: None,
		content: TypeContent::Enum(en),
		setters: Default::default(),
		methods: Default::default(),
	}
}

pub fn convert_item(input: &Item, wrappers: &HashMap<String, RustType>) -> RustType {
	match input {
		Item::Struct(s) => convert_struct(&s.ident, &s.fields, wrappers),
		Item::Enum(e) => convert_enum(&e.ident, &e.variants, wrappers),
		_ => panic!("Only structs or enums are supported"),
	}
}

fn type_to_str(ty: &Type) -> Option<String> {
	if let Type::Path(p) = ty {
		let segment = p.path.segments.last().unwrap();
		let segment = segment.value();
		let mut ident = segment.ident.to_string();
		if let PathArguments::AngleBracketed(a) = &segment.arguments {
			ident.push('<');
			for a in a.args.iter() {
				// Ignore lifetime arguments
				if let GenericArgument::Type(t) = a {
					if let Some(r) = type_to_str(t) {
						ident.push_str(&r);
					} else {
						return None;
					}
				}
			}
			if ident.ends_with('<') {
				ident.pop();
			} else {
				ident.push('>');
			}
		}
		Some(ident)
	} else if let Type::Reference(TypeReference { elem, .. }) = ty {
		type_to_str(elem)
	} else {
		None
	}
}

fn find_type<'a>(types: &'a [RustType], ty: &Type) -> Option<(usize, &'a RustType)> {
	let t = if let Some(r) = type_to_str(ty) {
		r
	} else {
		return None;
	};
	for (i, typ) in types.iter().enumerate() {
		if typ.name == t {
			return Some((i, typ));
		}
	}
	None
}

/// If a method is called `set_<name>` and takes `self` and one argument,
/// it is added as a setter.
/// It is also added for `<name>`, also taking `self` and another argument, if
/// `<name>` is a field.
pub fn add_indirect_set_method(types: &[RustType], wrappers: &HashMap<String, RustType>, typ: &RustType, meth: &MethodSig) -> Option<Setter> {
	let meth_name = meth.ident.to_string();
	if meth_name.starts_with("get_") {
		// Add indirect setters for nested structs
		let name = meth_name[4..].to_camel_case();
		match &meth.decl.output {
			ReturnType::Default => return None,
			ReturnType::Type(_, t) => if let Some(r) = type_to_str(t) {
				let r = if r.starts_with("Option<") {
					&r[7..r.len() - 1]
				} else {
					&r
				};

				if let Some(t) = types.iter().find(|t| t.name == r && !t.setters.is_empty()) {
					return Some(Setter {
						typ: t.clone(),
						field: name,
						setter: None,
					});
				}
			}
		}
	}
	None
}

/// If a method is called `set_<name>` and takes `self` and one argument,
/// it is added as a setter.
/// It is also added for `<name>`, also taking `self` and another argument, if
/// `<name>` is a field.
pub fn add_method(types: &[RustType], wrappers: &HashMap<String, RustType>, typ: &RustType, meth: &MethodSig) -> Option<Setter> {
	let meth_name = meth.ident.to_string();
	let name = if meth_name.starts_with("set_") {
		&meth_name[4..]
	} else if typ.get_all_fields().iter().any(|(p, n, _)| format!("{}{}", p, n.to_camel_case()) == meth_name) {
		&meth_name
	} else {
		return None;
	};

	let mut args = meth.decl.inputs.iter();
	if let Some(FnArg::SelfRef(_)) = args.next() {
	} else {
		return None;
	}
	let arg_type = match args.next() {
		Some(FnArg::Ignored(t)) => t,
		Some(FnArg::Captured(ArgCaptured { ty, .. })) => ty,
		_ => return None,
	};

	let arg_type = if let Some(r) = find_type(types, arg_type)
		.map(|(_, t)| Some(t.clone()))
		.unwrap_or_else(|| type_to_str(arg_type).map(|r| RustType::from_with_wrappers(r, wrappers))) {
		r
	} else {
		println!("Failed for {}", meth_name);
		return None;
	};

	// Only 2 arguments allowed
	if args.next().is_some() {
		return None;
	}

	let ret_type = match &meth.decl.output {
		ReturnType::Default => "()".into(),
		ReturnType::Type(_, t) =>
			find_type(types, t)
				.map(|(_, t)| Some(t.clone()))
				.unwrap_or_else(|| type_to_str(t).map(|r| RustType::from_with_wrappers(r, wrappers)))
				// Ignore return type if we cannot parse it
				.unwrap_or_else(|| "()".into()),
	};

	let setter = Setter {
		typ: arg_type,
		field: name.into(),
		setter: Some((meth_name, ret_type)),
	};
	Some(setter)
}

pub fn convert_file(input: &File, wrappers: &HashMap<String, RustType>) -> Vec<RustType> {
	let mut res = Vec::new();

	// Types
	for item in &input.items {
		match item {
			Item::Struct(_) => res.push(convert_item(item, wrappers)),
			Item::Enum(_) => res.push(convert_item(item, wrappers)),
			_ => {}
		}
	}

	// Setters and functions
	let mut changed = true;
	let mut iteration = 0;
	while changed {
		changed = false;
		for item in &input.items {
			match item {
				Item::Impl(ItemImpl { items, self_ty, .. }) => {
					let (i, typ) = if let Some(r) = find_type(&res, &**self_ty) {
						r
					} else {
						continue;
					};
					let mut setters = Vec::new();
					for item in items {
						if let ImplItem::Method(meth) = item {
							if let Some(s) = add_method(&res, wrappers, typ, &meth.sig) {
								setters.push(s);
							}
							if iteration >= 1 {
								if let Some(s) = add_indirect_set_method(&res, wrappers, typ, &meth.sig) {
									setters.push(s);
								}
							}
						}
					}

					for s in setters {
						if !res[i].setters.iter().any(|s2| s2.field == s.field) {
							res[i].setters.push(s);
							changed = true;
						}
					}
				}
				_ => {}
			}
		}
		iteration += 1;
	}

	res
}

/// Indent a string by a given count using tabs.
fn indent(s: &str, count: usize) -> String {
	let line_count = s.lines().count();
	let mut result = String::with_capacity(s.len() + line_count * count * 4);
	for l in s.lines() {
		if !l.is_empty() {
			result.push_str(
				std::iter::repeat("\t")
					.take(count)
					.collect::<String>()
					.as_str(),
			);
		}
		result.push_str(l);
		result.push('\n');
	}
	result
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn convert_u8() {
		let real: RustType = PrimitiveType::Int(false, Some(8)).into();
		assert_eq!(real, "u8".into());
	}
}
