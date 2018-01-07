extern crate regex;
#[macro_use]
extern crate t4rust_derive;

use std::env;
use std::fs::File;
use std::io::prelude::*;
use std::path::Path;

mod book_parser;

use book_parser::*;

#[derive(Template)]
#[TemplatePath = "build/FacadeDeclarations.tt"]
#[derive(Default, Debug)]
struct FacadeDeclarations(Declarations);

#[derive(Template)]
#[TemplatePath = "build/BookDeclarations.tt"]
#[derive(Default, Debug)]
struct Declarations {
    structs: Vec<Struct>,
    properties: Vec<Property>,
    nesteds: Vec<Nested>,
}

impl Declarations {
    fn get_property(&self, id: &str) -> &Property {
        println!("Get {} in {:?}", id, self.properties);
        let parts: Vec<_> = id.split('.').collect();
        self.properties.iter()
            .filter(|p| p.struct_name == parts[0] && p.name == parts[1])
            .next().expect("Unknown property")
    }
}

fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();

    // The template is automatically tracked as a dependency
    for f in &["BookDeclarations.txt"] {
        println!("cargo:rerun-if-changed={}/../declarations/{}", manifest_dir, f);
    }

    // Read declarations
    let mut f = File::open(&format!("{}/../declarations/BookDeclarations.txt", manifest_dir)).unwrap();
    let mut v = Vec::new();
    f.read_to_end(&mut v).unwrap();
    let s = String::from_utf8(v).unwrap();

    let decls = parse(&s);

    // Write declarations
    let out_dir = env::var("OUT_DIR").unwrap();
    let path = Path::new(&out_dir);
    let mut structs = File::create(&path.join("structs.rs")).unwrap();
    write!(&mut structs, "{}", decls).unwrap();

    // Write facades
    let mut structs = File::create(&path.join("facades.rs")).unwrap();
    write!(&mut structs, "{}", FacadeDeclarations(decls)).unwrap();
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

fn get_return_type(s: &str) -> String {
    if s.starts_with("Option<") {
        format!("Option<{}>", get_return_type(&s[7..s.len() - 1]))
    } else if s.starts_with("Vec<") {
        format!("Ref<[{}]>", &s[4..s.len() - 1])
    } else if s == "String" {
        String::from("Ref<str>")
    } else if is_ref_type(s) {
        format!("Ref<{}>", s)
    } else {
        String::from(s)
    }
}

fn get_id_args(ids: &[&Property], struc: &Struct) -> String {
    let mut res = String::new();
    for id in ids {
        if !res.is_empty() {
            res.push_str(", ");
        }
        res.push_str("self.");
        res.push_str(&id.get_attr_name(&struc.name));
    }
    res
}
