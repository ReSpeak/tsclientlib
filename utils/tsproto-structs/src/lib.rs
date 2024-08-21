//! `tsproto-structs` contains machine readable data for several TeamSpeak related topics.
//!
//! The underlying data files can be found in the
//! [tsdeclarations](https://github.com/ReSpeak/tsdeclarations) repository.

use std::fmt;
use std::str::FromStr;

use heck::*;
use serde::Deserialize;

type Result<T> = std::result::Result<T, fmt::Error>;

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

#[derive(Clone, Debug)]
pub enum InnerRustType {
	Primitive(String),
	Struct(String),
	Ref(Box<InnerRustType>),
	Option(Box<InnerRustType>),
	/// Key, value
	Map(Box<InnerRustType>, Box<InnerRustType>),
	Set(Box<InnerRustType>),
	Vec(Box<InnerRustType>),
	Cow(Box<InnerRustType>),
}

#[derive(Clone, Debug)]
pub struct RustType {
	pub inner: InnerRustType,
	/// Include a lifetime specifier 'a
	pub lifetime: bool,
}

impl InnerRustType {
	/// `lifetime`: Include 'a for references
	/// `is_ref`: If this is a refercene, used to emit either `str` or `String` or slices
	pub fn fmt(&self, f: &mut fmt::Formatter, lifetime: bool, is_ref: bool) -> fmt::Result {
		let lifetime_str = if lifetime { "'a " } else { "" };
		match self {
			Self::Struct(s) if s == "str" => {
				if is_ref {
					write!(f, "str")?;
				} else {
					write!(f, "String")?;
				}
			}
			Self::Struct(s) if s == "Uid" => {
				write!(f, "{}", s)?;
			}
			Self::Primitive(s) | Self::Struct(s) => write!(f, "{}", s)?,
			Self::Ref(i) => {
				write!(f, "&{}", lifetime_str)?;
				i.fmt(f, lifetime, true)?;
			}
			Self::Option(i) => {
				write!(f, "Option<")?;
				i.fmt(f, lifetime, false)?;
				write!(f, ">")?;
			}
			Self::Map(k, v) => {
				write!(f, "HashMap<")?;
				k.fmt(f, lifetime, false)?;
				write!(f, ", ")?;
				v.fmt(f, lifetime, false)?;
				write!(f, ">")?;
			}
			Self::Set(i) => {
				write!(f, "HashSet<")?;
				i.fmt(f, lifetime, false)?;
				write!(f, ">")?;
			}
			Self::Vec(i) => {
				if is_ref {
					write!(f, "[")?;
					i.fmt(f, lifetime, false)?;
					write!(f, "]")?;
				} else {
					write!(f, "Vec<")?;
					i.fmt(f, lifetime, false)?;
					write!(f, ">")?;
				}
			}
			Self::Cow(i) => {
				write!(f, "Cow<{}, ", lifetime_str.trim())?;
				i.fmt(f, false, true)?;
				write!(f, ">")?;
			}
		}
		Ok(())
	}

	/// Returns a converted type.
	pub fn to_ref(&self) -> Self {
		match self {
			Self::Struct(s) if s == "UidBuf" => Self::Ref(Box::new(Self::Struct("Uid".into()))),
			Self::Struct(s) if s == "String" => Self::Ref(Box::new(Self::Struct("str".into()))),
			Self::Struct(_) | Self::Map(_, _) | Self::Set(_) | Self::Vec(_) => {
				Self::Ref(Box::new(self.clone()))
			}
			Self::Primitive(_) | Self::Ref(_) | Self::Cow(_) => self.clone(),
			Self::Option(i) => Self::Option(Box::new(i.to_ref())),
		}
	}

	/// Returns a converted type.
	pub fn to_cow(&self) -> Self {
		match self {
			Self::Struct(s) if s == "UidBuf" => Self::Cow(Box::new(Self::Struct("Uid".into()))),
			Self::Struct(s) if s == "String" => Self::Cow(Box::new(Self::Struct("str".into()))),
			Self::Struct(_) | Self::Map(_, _) | Self::Set(_) | Self::Vec(_) => {
				Self::Cow(Box::new(self.clone()))
			}
			Self::Primitive(_) | Self::Ref(_) | Self::Cow(_) => self.clone(),
			Self::Option(i) => Self::Option(Box::new(i.to_cow())),
		}
	}

	/// Get code snippet for `as_ref`.
	pub fn code_as_ref(&self, name: &str) -> String {
		match self {
			Self::Struct(s) if s == "UidBuf" || s == "str" => format!("{}.as_ref()", name),
			Self::Struct(s) if s == "String" => format!("{}.as_str()", name),
			Self::Struct(s) if s == "str" => name.into(),
			Self::Struct(_) => format!("&{}", name),
			Self::Map(_, _) | Self::Set(_) | Self::Vec(_) | Self::Cow(_) => {
				format!("{}.as_ref()", name)
			}
			Self::Primitive(_) => name.into(),
			Self::Ref(i) => {
				let inner = i.code_as_ref(name);
				if inner == name {
					format!("*{}", name)
				} else if inner.starts_with('&') && &inner[1..] == name {
					name.into()
				} else {
					inner
				}
			}
			Self::Option(i) => {
				match &**i {
					// Shortcut
					Self::Struct(s) if s == "String" => format!("{}.as_deref()", name),
					_ => {
						let inner = i.code_as_ref(name);
						if inner == name {
							inner
						} else if inner.starts_with('&') && &inner[1..] == name {
							format!("{0}.as_ref()", name)
						} else {
							format!("{0}.as_ref().map(|{0}| {1})", name, inner)
						}
					}
				}
			}
		}
	}

	pub fn uses_lifetime(&self) -> bool {
		match self {
			Self::Struct(s) if s == "Uid" => true,
			Self::Ref(_) | Self::Cow(_) => true,
			Self::Map(_, i) | Self::Set(i) | Self::Vec(i) | Self::Option(i) => i.uses_lifetime(),
			_ => false,
		}
	}
}

impl FromStr for InnerRustType {
	type Err = fmt::Error;
	fn from_str(s: &str) -> Result<Self> {
		if s == "DateTime" {
			Ok(Self::Primitive("OffsetDateTime".into()))
		} else if s.starts_with("Duration") {
			Ok(Self::Primitive("Duration".into()))
		} else if s == "PermissionId" {
			Ok(Self::Primitive("Permission".into()))
		} else if s == "Ts3ErrorCode" {
			Ok(Self::Primitive("Error".into()))
		} else if s == "bool"
			|| s.starts_with('i')
			|| s.starts_with('u')
			|| s.starts_with('f')
			|| s.ends_with("Id")
			|| s.ends_with("Type")
			|| s.ends_with("Mode")
			|| s == "ChannelPermissionHint"
			|| s == "ClientPermissionHint"
			|| s == "Codec"
			|| s == "LogLevel"
			|| s == "MaxClients"
			|| s == "IpAddr"
			|| s == "Reason"
			|| s == "SocketAddr"
		{
			Ok(Self::Primitive(s.into()))
		} else if s == "Uid" {
			Ok(Self::Struct("UidBuf".into()))
		} else if s == "str" || s == "String" {
			Ok(Self::Struct("String".into()))
		} else if let Some(rest) = s.strip_prefix('&') {
			let rest = if rest.starts_with('\'') {
				let i = rest.find(' ').ok_or_else(|| {
					eprintln!("Reference type with lifetime has no inner type: {:?}", s);
					fmt::Error
				})?;
				&rest[i + 1..]
			} else {
				rest
			};
			Ok(Self::Ref(Box::new(rest.parse()?)))
		} else if let Some(rest) = s.strip_suffix('?') {
			Ok(Self::Option(Box::new(rest.parse()?)))
		} else if s.starts_with("Option<") {
			let rest = &s[7..s.len() - 1];
			Ok(Self::Option(Box::new(rest.parse()?)))
		} else if let Some(rest) = s.strip_suffix("[]") {
			Ok(Self::Vec(Box::new(rest.parse()?)))
		} else if s.starts_with("HashMap<") {
			let rest = &s[8..s.len() - 1];
			let i = rest.find(',').ok_or_else(|| {
				eprintln!("HashMap without key: {:?}", s);
				fmt::Error
			})?;
			Ok(Self::Map(Box::new(rest[..i].parse()?), Box::new(rest[i..].trim().parse()?)))
		} else if s.starts_with("HashSet<") {
			let rest = &s[8..s.len() - 1];
			Ok(Self::Set(Box::new(rest.parse()?)))
		} else if s.starts_with("Vec<") {
			let rest = &s[4..s.len() - 1];
			Ok(Self::Vec(Box::new(rest.parse()?)))
		} else if s.starts_with('[') {
			let rest = &s[1..s.len() - 1];
			if rest.contains(';') {
				// Slice with explicit length, take as struct
				Ok(Self::Struct(s.into()))
			} else {
				Ok(Self::Vec(Box::new(rest.parse()?)))
			}
		} else if let Some(rest) = s.strip_suffix('T') {
			rest.parse()
		} else {
			Ok(Self::Struct(s.into()))
		}
	}
}

impl FromStr for RustType {
	type Err = fmt::Error;
	fn from_str(s: &str) -> Result<Self> {
		Ok(Self { inner: s.parse()?, lifetime: s.contains("&'") })
	}
}

impl RustType {
	pub fn with_opt(s: &str, opt: bool) -> Result<Self> {
		let inner = s.parse()?;
		let inner = if opt { InnerRustType::Option(Box::new(inner)) } else { inner };
		Ok(Self { inner, lifetime: s.contains("&'") })
	}

	/// `map` has a key
	pub fn with(s: &str, opt: bool, map: Option<&str>, set: bool, vec: bool) -> Result<Self> {
		assert!(
			[map.is_some(), set, vec].iter().filter(|b| **b).count() <= 1,
			"Too many modifiers active (map: {:?}, set: {:?}, vec: {:?})",
			map,
			set,
			vec
		);
		let mut inner = s.parse()?;
		if let Some(key) = map {
			inner = InnerRustType::Map(Box::new(key.parse()?), Box::new(inner));
		}
		if set {
			inner = InnerRustType::Set(Box::new(inner));
		}
		if vec {
			inner = InnerRustType::Vec(Box::new(inner));
		}

		inner = if opt { InnerRustType::Option(Box::new(inner)) } else { inner };
		Ok(Self { inner, lifetime: s.contains("&'") })
	}

	pub fn to_opt(&self, opt: bool) -> Self {
		if opt {
			Self {
				inner: InnerRustType::Option(Box::new(self.inner.clone())),
				lifetime: self.lifetime,
			}
		} else {
			self.clone()
		}
	}

	pub fn to_ref(&self, as_ref: bool) -> Self {
		if as_ref {
			Self { inner: self.inner.to_ref(), lifetime: self.lifetime }
		} else {
			self.clone()
		}
	}

	pub fn to_cow(&self) -> Self { Self { inner: self.inner.to_cow(), lifetime: self.lifetime } }

	pub fn lifetime(&self, lifetime: bool) -> Self {
		let mut r = self.clone();
		r.lifetime = lifetime;
		r
	}

	pub fn wrap_ref(&self) -> Self {
		Self { inner: InnerRustType::Ref(Box::new(self.inner.clone())), lifetime: self.lifetime }
	}

	pub fn wrap_opt(&self) -> Self {
		Self { inner: InnerRustType::Option(Box::new(self.inner.clone())), lifetime: self.lifetime }
	}

	pub fn is_opt(&self) -> bool { matches!(self.inner, InnerRustType::Option(_)) }

	pub fn is_primitive(&self) -> bool {
		let inner = if let InnerRustType::Option(t) = &self.inner { t } else { &self.inner };
		matches!(inner, InnerRustType::Primitive(_))
	}

	pub fn is_vec(&self) -> bool { matches!(self.inner, InnerRustType::Vec(_)) }

	pub fn is_cow(&self) -> bool {
		let inner = if let InnerRustType::Option(i) = &self.inner { i } else { &self.inner };
		matches!(inner, InnerRustType::Cow(_))
	}

	pub fn uses_lifetime(&self) -> bool { self.inner.uses_lifetime() }

	/// Returns an identifier from this type in camelCase.
	pub fn to_name(&self) -> String {
		self.to_string().replace('<', "_").replace('>', "").to_upper_camel_case()
	}

	/// Get code snippet for `as_ref`.
	pub fn code_as_ref(&self, name: &str) -> String { self.inner.code_as_ref(name) }
}

impl From<InnerRustType> for RustType {
	fn from(inner: InnerRustType) -> Self { Self { inner, lifetime: false } }
}

impl fmt::Display for RustType {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result { self.inner.fmt(f, self.lifetime, false) }
}

fn get_false() -> bool { false }

/// Prepend `/// ` to each line of a string.
pub fn doc_comment(s: &str) -> String { s.lines().map(|l| format!("/// {}\n", l)).collect() }

/// Indent a string by a given count using tabs.
pub fn indent<S: AsRef<str>>(s: S, count: usize) -> String {
	let sref = s.as_ref();
	let line_count = sref.lines().count();
	let mut result = String::with_capacity(sref.len() + line_count * count * 4);
	for l in sref.lines() {
		if !l.is_empty() {
			result.push_str(&"\t".repeat(count));
		}
		result.push_str(l);
		result.push('\n');
	}
	result
}

/// Unindent a string by a given count of tabs.
pub fn unindent(s: &mut String) {
	std::mem::swap(&mut s.replace("\n\t", "\n"), s);
	if s.get(0..1) == Some("\t") {
		s.remove(0);
	}
}

/// Returns an empty string if `s` is empty, otherwise `s` with braces.
pub fn embrace(s: &str) -> String { if s.is_empty() { String::new() } else { format!("({})", s) } }
