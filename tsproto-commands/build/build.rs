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

fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();

    // The template is automatically tracked as a dependency
    for f in &["build/build.rs", "../declarations/MessageDeclarations.txt"] {
        println!("cargo:rerun-if-changed={}/{}", manifest_dir, f);
    }

    // Read declarations
    let mut f = File::open(&format!("{}/../declarations/MessageDeclarations.txt", manifest_dir)).unwrap();
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
