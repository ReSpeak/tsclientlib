extern crate tsproto_util;

use std::env;
use std::fs::File;
use std::io::prelude::*;
use std::path::Path;

use tsproto_util::{Declaration, MessageDeclarations, Permissions, Errors};

fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let base_dir = format!("{}/..", manifest_dir);
    let out_dir = env::var("OUT_DIR").unwrap();

    // Read errors
    let decls = Errors::from_file(&base_dir);

    // Write errors
    let path = Path::new(&out_dir);
    let mut structs = File::create(&path.join("errors.rs")).unwrap();
    write!(&mut structs, "{}", decls).unwrap();

    // Read permissions
    let decls = Permissions::from_file(&base_dir);

    // Write permissions
    let path = Path::new(&out_dir);
    let mut structs = File::create(&path.join("permissions.rs")).unwrap();
    write!(&mut structs, "{}", decls).unwrap();

    // Read messages
    let decls = MessageDeclarations::from_file(&base_dir);

    // Write messages
    let path = Path::new(&out_dir);
    let mut structs = File::create(&path.join("messages.rs")).unwrap();
    write!(&mut structs, "{}", decls).unwrap();
}