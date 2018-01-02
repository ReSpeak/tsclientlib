use std;

use regex::{CaptureMatches, Regex};

use ::Declarations;

#[derive(Default, Clone, Debug)]
pub struct Values {
    pub doc: String,
    pub get: Option<bool>,
    pub set: Option<bool>,
    pub id: String,
    pub optional: Option<bool>,
}

impl Values {
    pub fn fill<'r, 't>(&mut self, captures: CaptureMatches<'r, 't>) {
        for cap in captures {
            let val = cap["pval"].trim();
            let key = cap["pname"].to_lowercase();
            match key.as_str() {
                "doc" => self.doc = unquote(val),
                "get" => self.get = Some(val.parse().unwrap()),
                "set" => self.set = Some(val.parse().unwrap()),
                "id" => self.id = val.to_string(),
                "optional" => self.optional = Some(val.parse().unwrap()),
                _ => {
                    panic!("Invalid value '{}'", key);
                }
            }
        }
    }
}

#[derive(Default, Clone, Debug)]
pub struct Struct {
    pub name: String,
    pub values: Values,
}

#[derive(Default, Clone, Debug)]
pub struct Property {
    pub name: String,
    pub type_s: String,
    pub values: Values,
    pub struct_name: String,
}

impl Property {
    pub fn get_attr_name(&self, struct_name: &str) -> String {
        if self.struct_name == struct_name {
            to_snake_case(&self.name)
        } else {
            format!("{}_{}", to_snake_case(&self.struct_name), to_snake_case(&self.name))
        }
    }
}

impl PartialEq for Property {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && self.struct_name == other.struct_name
    }
}

impl Eq for Property {}

pub(crate) fn parse(s: &str) -> Declarations {
    let param_re = Regex::new(r#"\s*(?P<pname>(get|set|doc|id|optional))\s*:\s*(?P<pval>(?:\w+|"([^"]|["\\n])*"|\[[^]]*\]))\s*,?"#).unwrap();
    let struct_re = Regex::new(r"\s*(?P<name>\w+)\s*;?").unwrap();
    let prop_re = Regex::new(r"\s*(?P<name>\w+)\s*,\s*(?P<type>\w+)(?P<mod>(\?|\[\])?)\s*;?").unwrap();

    let mut decls = Declarations::default();
    let mut cur_struct_name = None;
    let mut default_vals = Values::default();

    for l in s.lines() {
        if l.trim().is_empty() {
            continue;
        }

        let parts: Vec<_> = l.splitn(2, ':').collect();
        if parts.len() < 2 {
            continue;
        }
        let type_s = parts[0].trim().to_uppercase();
        match type_s.as_str() {
            "STRUCT" => {
                let capture = struct_re.captures(parts[1]).expect("No match found");
                let end = capture[0].len();

                let captures = param_re.captures_iter(&parts[1][end..]);
                let mut vals = Values::default();
                vals.fill(captures);

                let new_struct = Struct {
                    name: capture["name"].to_string(),
                    values: vals,
                };
                cur_struct_name = Some(new_struct.name.clone());
                decls.structs.push(new_struct);
            }
            "PROP" => {
                let capture = prop_re.captures(parts[1]).expect("No match found");
                let end = capture[0].len();

                let captures = param_re.captures_iter(&parts[1][end..]);
                let mut vals = default_vals.clone();
                vals.fill(captures);

                let is_array = &capture["mod"] == "[]";
                let is_optional = &capture["mod"] == "?";
                let mut type_s = convert_type(&capture["type"]);

                if is_array {
                    type_s = format!("Vec<{}>", type_s);
                }
                if is_optional {
                    type_s = format!("Option<{}>", type_s);
                }

                let prop = Property {
                    name: capture["name"].to_string(),
                    type_s,
                    values: vals,
                    struct_name: cur_struct_name.as_ref()
                        .expect("No struct known").clone(),
                };
                decls.properties.push(prop);
            }
            "DEFAULT" => {
                let captures = param_re.captures_iter(parts[1]);
                default_vals = Values::default();
                default_vals.fill(captures);
            }
            "NESTED" => {
            }
            "" => {
                continue;
            }
            _ => {
                panic!("Invalid type '{}'", parts[0].trim());
            }
        }
    }

    decls
}

pub fn convert_type(t: &str) -> String {
    if t == "str" {
        String::from("String")
    } else if t == "DateTime" {
        String::from("DateTime<Utc>")
    } else if t == "TimeSpan" {
        String::from("Duration")
    } else {
        t.into()
    }
}

#[allow(dead_code)]
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

pub fn unquote(s: &str) -> String {
    if !s.starts_with('"') || !s.ends_with('"') {
        return s.to_string();
    }
    let s = &s[1..(s.len() - 1)];
    s.replace("\\n", "\n").replace("\\\"", "\"").replace("\\\\", "\\")
}

pub fn document(s: &str) -> String {
    s.lines().map(|l| format!("/// {}\n", l)).collect()
}

/// Indent a string by a given count using tabs.
pub fn indent<S: AsRef<str>>(s: S, count: usize) -> String {
    let sref = s.as_ref();
    let line_count = sref.lines().count();
    let mut result = String::with_capacity(sref.len() + line_count * count * 4);
    for l in sref.lines() {
        if !l.is_empty() {
            result.push_str(std::iter::repeat("    ").take(count).collect::<String>().as_str());
        }
        result.push_str(l);
        result.push('\n');
    }
    result
}
