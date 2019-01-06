use serde_derive::Deserialize;

pub mod book;
pub mod book_to_messages;
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

fn to_snake_case<S: AsRef<str>>(text: S) -> String {
	let sref = text.as_ref();
	let mut s = String::with_capacity(sref.len());
	for c in sref.chars() {
		if c.is_uppercase() {
			if !s.is_empty() {
				s.push('_');
			}
			s.push_str(&c.to_lowercase().to_string());
		} else {
			s.push(c);
		}
	}
	s
}

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

fn get_false() -> bool {
	false
}

/// Unindent a string by a given count of tabs.
pub fn unindent(mut s: &mut String) {
	std::mem::swap(&mut s.replace("\n\t", "\n"), &mut s);
	if s.get(0..1) == Some("\t") {
		s.remove(0);
	}
}
