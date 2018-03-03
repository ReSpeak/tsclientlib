extern crate csv;
extern crate regex;
#[macro_use]
extern crate t4rust_derive;
#[macro_use]
extern crate serde_derive;

use std::io::{Cursor};
use std::io::prelude::*;
use std::fs::File;

mod book_parser;
mod error_parser;
mod message_parser;
mod permission_parser;
mod messages_to_book_parser;
mod facade_parser;

pub use book_parser::BookDeclarations;
pub use message_parser::MessageDeclarations;
pub use error_parser::Errors;
pub use permission_parser::Permissions;
pub use messages_to_book_parser::MessagesToBook;
pub use facade_parser::FacadeDeclarations;

type Map<K, V> = std::collections::HashMap<K, V>;

#[derive(Debug, Deserialize)]
pub struct EnumValue {
    pub name: String,
    pub doc: String,
    pub num: String,
}

pub trait Declaration {
    fn get_filename() -> &'static str;

    fn parse(s: &str) -> Self where Self: Sized {
        let mut cursor = Cursor::new(s.as_bytes());
        Self::parse_from_read(&mut cursor)
    }

    fn parse_from_read(read: &mut Read) -> Self where Self: Sized {
        let mut v = Vec::new();
        read.read_to_end(&mut v).unwrap();
        let s = String::from_utf8(v).unwrap();
        Self::parse(&s)
    }

    fn from_file(base: &str) -> Self where Self: Sized {
        let file = format!("{}/declarations/{}", base, Self::get_filename());
        println!("cargo:rerun-if-changed={}", file);

        let mut fread = File::open(file).unwrap();
        Self::parse_from_read(&mut fread)
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