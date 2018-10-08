use book_parser::*;
use *;

#[derive(Template)]
#[TemplatePath = "src/FacadeDeclarations.tt"]
#[derive(Debug)]
pub struct FacadeDeclarations(pub BookDeclarations);

fn get_return_type(s: &str) -> String {
	if s.starts_with("Option<") {
		format!("Option<{}>", get_return_type(&s[7..s.len() - 1]))
	} else if s.starts_with("Vec<") {
		format!("&mut [{}]", &s[4..s.len() - 1])
	} else if s == "String" {
		String::from("&str")
	} else if is_ref_type(s) {
		format!("&mut {}", s)
	} else {
		String::from(s)
	}
}

fn get_id_args(ids: &[Id], structs: &[Struct], struc: &Struct) -> String {
	let mut res = String::new();
	for id in ids {
		let p = id.find_property(structs);
		if !res.is_empty() {
			res.push_str(", ");
		}
		if is_ref_type(&p.type_s) {
			res.push('&');
		}
		res.push_str("self.");
		res.push_str(&PropId::from(&*id).get_attr_name(struc));
	}
	res
}
