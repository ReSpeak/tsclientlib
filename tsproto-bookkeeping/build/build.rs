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
#[TemplatePath = "build/BookDeclarations.tt"]
#[derive(Default, Debug)]
struct Declarations {
    structs: Vec<Struct>,
    properties: Vec<Property>,
}

impl Declarations {
    fn get_property(&self, id: &str) -> &Property {
        let parts: Vec<_> = id.split('.').collect();
        self.properties.iter()
            .filter(|p| p.struct_name == parts[0] && p.name == parts[1])
            .next().expect("Unknown property")
    }
}

fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();

    // The template is automatically tracked as a dependency
    for f in &["build/build.rs", "../declarations/BookDeclarations.txt"] {
        println!("cargo:rerun-if-changed={}/{}", manifest_dir, f);
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
}
