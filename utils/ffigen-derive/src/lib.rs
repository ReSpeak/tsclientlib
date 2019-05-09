//! Create `ffigen::RustType`s from code.
extern crate proc_macro;

use std::collections::HashMap;
use std::str::FromStr;

use ffigen::*;
use proc_macro::TokenStream;
use syn::*;
use quote::ToTokens;

#[proc_macro_derive(FfiGen, attributes(FfiGenWrapper))]
pub fn gen_ffi(input: TokenStream) -> TokenStream {
	let macro_input = parse_macro_input!(input as DeriveInput);

	let mut wrappers = HashMap::new();
	// TODO Remove Special types
	wrappers.insert("ConnectionId".into(), "u64".into());
	wrappers.insert("FutureHandle".into(), "u64".into());
	wrappers.insert("MessageTarget".into(), "u8".into());

	for attr in &macro_input.attrs {
		if let Ok(Meta::List(MetaList { ident, nested, .. })) = attr.parse_meta() {
			if ident.to_string() == "FfiGenWrapper" {
				if nested.len() < 1 {
					panic!("Wrong length for FfiGenWrapper");
				}
				let nested = nested.first().unwrap().into_value();
				if let NestedMeta::Meta(_meta) = nested {
					// TODO Parsing is quite complicated
					//wrappers.insert("".to_string(), lit_str.value().into());
				}
				// TODO Support from and to directives
			}
		}
	}

	let ty = convert(macro_input, &wrappers);
	let res = ty.to_string();

	//use std::io::Write;
	//let mut file = std::fs::OpenOptions::new().append(true).open("/tmp/ffigen-output.rs").unwrap();
	//write!(file, "{}", t).unwrap();
	//std::fs::write("/tmp/ffigen-output.rs", res.as_bytes()).unwrap();
	match proc_macro::TokenStream::from_str(&res) {
		Ok(r) => r,
		Err(e) => {
			panic!("Failed to parse output as rust code: {:?}", e);
		}
	}
}

fn fields_to_struct(fields: &Fields, wrappers: &HashMap<String, RustType>) -> Struct {
	let mut struc = Struct { fields: Vec::new() };
	match fields {
		Fields::Named(n) => for f in n.named.iter() {
			let n = f.ident.as_ref().map(|i| i.to_string()).unwrap_or_else(String::new);
			let ty = f.ty.clone().into_token_stream().to_string();
			struc.fields.push((n, RustType::from_with_wrappers(&ty, wrappers)));
		}
		Fields::Unnamed(n) => for f in n.unnamed.iter() {
			let n = f.ident.as_ref().map(|i| i.to_string()).unwrap_or_else(String::new);
			let ty = f.ty.clone().into_token_stream().to_string();
			struc.fields.push((n, RustType::from_with_wrappers(&ty, wrappers)));
		}
		Fields::Unit => {}
	}
	struc
}

fn convert(input: DeriveInput, wrappers: &HashMap<String, RustType>) -> RustType {
	let name = input.ident.to_string();
	match input.data {
		Data::Struct(s) => {
			RustType {
				name,
				wrapper: None,
				content: TypeContent::Struct(fields_to_struct(&s.fields, wrappers)),
			}
		}
		Data::Enum(e) => {
			let mut en = Enum { possibilities: Vec::new() };
			for v in e.variants {
				let prefix = v.ident.to_string();
				en.possibilities.push((prefix, fields_to_struct(&v.fields, wrappers)));
			}
			RustType {
				name,
				wrapper: None,
				content: TypeContent::Enum(en),
			}
		}
		_ => panic!("Only structs or enums are supported"),
	}
}
