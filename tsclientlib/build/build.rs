extern crate tsproto_util;

use std::env;
use std::fs::File;
use std::io::prelude::*;
use std::path::Path;

use tsproto_util::{Declaration, BookDeclarations, FacadeDeclarations, MessageDeclarations, MessagesToBookDeclarations};

fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let base_dir = format!("{}/..", manifest_dir);
    let out_dir = env::var("OUT_DIR").unwrap();

    // Read declarations
    let decls = BookDeclarations::from_file(&base_dir, ());

    // Write declarations
    let path = Path::new(&out_dir);
    let mut structs = File::create(&path.join("structs.rs")).unwrap();
    write!(&mut structs, "{}", decls).unwrap();

    let messages = MessageDeclarations::from_file(&base_dir, ());

    {
        let m2bdecls = MessagesToBookDeclarations::from_file(&base_dir, (&decls, &messages));
    }

    // Write facades
    let mut structs = File::create(&path.join("facades.rs")).unwrap();
    write!(&mut structs, "{}", FacadeDeclarations(decls)).unwrap();
}
