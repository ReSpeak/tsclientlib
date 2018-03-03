use ::*;
use book_parser::*;

#[derive(Template)]
#[TemplatePath = "src/FacadeDeclarations.tt"]
#[derive(Default, Debug)]
pub struct FacadeDeclarations(pub BookDeclarations);

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
        if is_ref_type(&id.type_s) {
            res.push('&');
        }
        res.push_str("self.");
        res.push_str(&id.get_attr_name(&struc.name));
    }
    res
}
