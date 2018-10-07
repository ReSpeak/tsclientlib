extern crate base64;
extern crate csv;
extern crate regex;
#[macro_use]
extern crate t4rust_derive;
extern crate toml;
extern crate serde;
#[macro_use]
extern crate serde_derive;

use std::io::{Cursor};
use std::io::prelude::*;
use std::fs::File;

mod book_parser;
mod error_parser;
mod facade_parser;
mod message_parser;
mod messages_to_book_parser;
mod packets_parser;
mod permission_parser;
mod version_parser;

pub use book_parser::BookDeclarations;
pub use error_parser::Errors;
pub use facade_parser::FacadeDeclarations;
pub use message_parser::MessageDeclarations;
pub use messages_to_book_parser::MessagesToBookDeclarations;
pub use packets_parser::Packets;
pub use permission_parser::Permissions;
pub use version_parser::Versions;

#[derive(Debug, Deserialize)]
pub struct EnumValue {
    pub name: String,
    pub doc: String,
    pub num: String,
}

pub trait Declaration {
    type Dep;

    fn get_filename() -> &'static str;

    fn parse(s: &str, dep: Self::Dep) -> Self where Self: Sized {
        let mut cursor = Cursor::new(s.as_bytes());
        Self::parse_from_read(&mut cursor, dep)
    }

    fn parse_from_read(read: &mut Read, dep: Self::Dep) -> Self where Self: Sized {
        let mut v = Vec::new();
        read.read_to_end(&mut v).unwrap();
        let s = String::from_utf8(v).unwrap();
        Self::parse(&s, dep)
    }

    fn from_file(base: &str, dep: Self::Dep) -> Self where Self: Sized {
        let file = format!("{}/declarations/{}", base, Self::get_filename());
        println!("cargo:rerun-if-changed={}", file);

        let mut fread = File::open(file).unwrap();
        Self::parse_from_read(&mut fread, dep)
    }
}

pub fn to_pascal_case<S: AsRef<str>>(text: S) -> String {
    let sref = text.as_ref();
    let mut s = String::with_capacity(sref.len());
    let mut uppercase = true;
    for c in sref.chars() {
        if c == '_' {
            uppercase = true;
        } else {
            if uppercase {
                s.push(c.to_uppercase().next().unwrap());
                uppercase = false;
            } else {
                s.push(c);
            }
        }
    }
    s
}

pub fn to_snake_case<S: AsRef<str>>(text: S) -> String {
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

fn is_ref_type(s: &str) -> bool {
    if s.starts_with("Option<") {
        is_ref_type(&s[7..s.len() - 1])
    } else {
        !(s == "bool" || s.starts_with("i") || s.starts_with("u")
            || s.starts_with("f") || s.ends_with("Id") || s.ends_with("Type")
            || s.ends_with("Mode"))
    }
}

/// Prepend `/// ` to each line of a string.
pub fn doc_comment(s: &str) -> String {
    s.lines().map(|l| format!("/// {}\n", l)).collect()
}

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
fn unindent(mut s: &mut String) {
    std::mem::swap(&mut s.replace("\n\t", "\n"), &mut s);
    if s.get(0..1) == Some("\t") {
        s.remove(0);
    }
}

pub fn join<S: AsRef<str>, S2: AsRef<str>, I: Iterator<Item = S>>(i: I, joiner: S2) -> String {
    let joiner = joiner.as_ref();
    let mut res = String::new();
    for e in i {
        if !res.is_empty() {
            res.push_str(joiner);
        }
        res.push_str(e.as_ref());
    }
    res
}

pub fn unquote(s: &str) -> String {
    if !s.starts_with('"') || !s.ends_with('"') {
        return s.to_string();
    }
    let s = &s[1..(s.len() - 1)];
    s.replace("\\n", "\n").replace("\\\"", "\"").replace("\\\\", "\\")
}

pub fn get_false() -> bool {
    false
}
