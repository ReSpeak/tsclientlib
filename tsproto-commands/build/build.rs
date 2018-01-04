extern crate csv;
extern crate serde;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate t4rust_derive;

use std::env;
use std::fs::File;
use std::io::prelude::*;
use std::path::Path;

mod message_parser;

use message_parser::*;

type Map<K, V> = std::collections::HashMap<K, V>;

#[derive(Template)]
#[TemplatePath = "build/MessageDeclarations.tt"]
#[derive(Default, Debug)]
struct Declarations {
    fields: Map<String, Field>,
    messages: Map<String, Message>,
    notifies: Map<String, Notify>,
}

#[derive(Template)]
#[TemplatePath = "build/ErrorDeclarations.tt"]
#[derive(Default, Debug)]
struct Errors(Vec<EnumValue>);

#[derive(Template)]
#[TemplatePath = "build/PermissionDeclarations.tt"]
#[derive(Default, Debug)]
struct Permissions(Vec<EnumValue>);

#[derive(Debug, Deserialize)]
pub struct EnumValue {
    pub name: String,
    pub doc: String,
    pub num: String,
}

fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let out_dir = env::var("OUT_DIR").unwrap();

    // The template is automatically tracked as a dependency
    for f in &["Errors.csv", "Messages.txt", "Permissions.csv"] {
        println!("cargo:rerun-if-changed={}/../declarations/{}", manifest_dir, f);
    }

    // Read errors
    let mut table = csv::Reader::from_reader(File::open(
        &format!("{}/../declarations/Errors.csv", manifest_dir))
        .unwrap());
    let decls = Errors(table.deserialize().collect::<Result<Vec<_>, _>>().unwrap());

    // Write errors
    let path = Path::new(&out_dir);
    let mut structs = File::create(&path.join("errors.rs")).unwrap();
    write!(&mut structs, "{}", decls).unwrap();

    // Read permissions
    let mut table = csv::Reader::from_reader(File::open(
        &format!("{}/../declarations/Permissions.csv", manifest_dir))
        .unwrap());
    let decls = Permissions(table.deserialize().collect::<Result<Vec<_>, _>>().unwrap());

    // Write permissions
    let path = Path::new(&out_dir);
    let mut structs = File::create(&path.join("permissions.rs")).unwrap();
    write!(&mut structs, "{}", decls).unwrap();

    // Read messages
    let mut f = File::open(&format!("{}/../declarations/Messages.txt", manifest_dir)).unwrap();
    let mut v = Vec::new();
    f.read_to_end(&mut v).unwrap();
    let s = String::from_utf8(v).unwrap();
    let decls = message_parser::parse(&s);

    // Write messages
    let path = Path::new(&out_dir);
    let mut structs = File::create(&path.join("messages.rs")).unwrap();
    write!(&mut structs, "{}", decls).unwrap();
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

fn to_pascal_case(text: &str) -> String {
    let mut s = String::with_capacity(text.len());
    let mut uppercase = true;
    for c in text.chars() {
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

/// Prepend `/// ` to each line of a string.
fn doc_comment(s: &str) -> String {
    let line_count = s.lines().count();
    let mut result = String::with_capacity(s.len() + line_count * 4);
    for l in s.lines() {
        if !l.is_empty() {
            result.push_str("/// ");
        }
        result.push_str(l);
        result.push('\n');
    }
    result
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
