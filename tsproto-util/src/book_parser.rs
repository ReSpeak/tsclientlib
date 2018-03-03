use regex::{CaptureMatches, Regex};
use ::*;

#[derive(Template)]
#[TemplatePath = "src/BookDeclarations.tt"]
#[derive(Default, Debug)]
pub struct BookDeclarations {
    pub(crate) structs: Vec<Struct>,
    pub(crate) properties: Vec<Property>,
    pub(crate) nesteds: Vec<Nested>,
}

impl Declaration for BookDeclarations {
    type Dep = ();

    fn get_filename() -> &'static str { "BookDeclarations.txt" }

    fn parse(s: &str, (): Self::Dep) -> Self {
        let param_re = Regex::new(r#"\s*(?P<pname>(get|set|doc|id))\s*:\s*(?P<pval>(?:\w+|"([^"]|["\\n])*"|\[[^]]*\]))\s*,?"#).unwrap();
        let struct_re = Regex::new(r"\s*(?P<name>\w+)\s*;?").unwrap();
        let prop_re = Regex::new(r"\s*(?P<name>\w+)\s*,\s*(?P<type>\w+)(?P<mod>(\?|\[\])?)\s*;?").unwrap();

        let mut decls = BookDeclarations::default();
        let mut cur_struct_name = None;
        let mut default_vals = Values::default();

        for (i, l) in s.lines().enumerate() {
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
                    let capture = struct_re.captures(parts[1]).expect(&format!("No match found in line {}", i + 1));
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
                "NESTED" |
                "PROP" => {
                    let is_prop = type_s == "PROP";

                    let capture = prop_re.captures(parts[1]).expect(&format!("No match found in line {}", i + 1));
                    let end = capture[0].len();

                    let captures = param_re.captures_iter(&parts[1][end..]);
                    let mut vals = default_vals.clone();
                    vals.fill(captures);

                    let is_array = &capture["mod"] == "[]";
                    let is_optional = &capture["mod"] == "?";
                    let mut type_s = convert_type(&capture["type"]);

                    if is_array {
                        match type_s.as_str() {
                            "Client" | "Channel" | "ServerGroup" =>
                                type_s = format!("Map<{0}Id, {0}>", type_s),
                            _ => type_s = format!("Vec<{}>", type_s),
                        }
                    }
                    if is_optional {
                        type_s = format!("Option<{}>", type_s);
                    }

                    if is_prop {
                        let prop = Property {
                            name: capture["name"].to_string(),
                            type_s,
                            values: vals,
                            struct_name: cur_struct_name.as_ref()
                                .expect("No struct known").clone(),
                        };
                        decls.properties.push(prop);
                    } else {
                        // NESTED
                        let prop = Nested {
                            name: capture["name"].to_string(),
                            type_s,
                            values: vals,
                            struct_name: cur_struct_name.as_ref()
                                .expect("No struct known").clone(),
                        };
                        decls.nesteds.push(prop);
                    }
                }
                "DEFAULT" => {
                    let captures = param_re.captures_iter(parts[1]);
                    default_vals = Values::default();
                    default_vals.fill(captures);
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
}

impl BookDeclarations {
    pub(crate) fn get_property(&self, id: &str) -> &Property {
        println!("Get {} in {:?}", id, self.properties);
        let parts: Vec<_> = id.split('.').collect();
        self.properties.iter()
            .filter(|p| p.struct_name == parts[0] && p.name == parts[1])
            .next().expect("Unknown property")
    }
}

#[derive(Default, Clone, Debug)]
pub struct Values {
    pub doc: String,
    pub get: Option<bool>,
    pub set: Option<bool>,
    pub id: String,
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
    /// The name of this property (in PascalCase) which can be called from rust when generated.
    pub name: String,
    /// The rust declaration type.
    pub type_s: String,
    pub values: Values,
    /// The name of the struct which is containing this property.
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

#[derive(Default, Clone, Debug)]
pub struct Nested {
    pub name: String,
    pub type_s: String,
    pub values: Values,
    pub struct_name: String,
}

impl Nested {
    pub fn get_attr_name(&self, struct_name: &str) -> String {
        if self.struct_name == struct_name {
            to_snake_case(&self.name)
        } else {
            format!("{}_{}", to_snake_case(&self.struct_name), to_snake_case(&self.name))
        }
    }
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

pub fn unquote(s: &str) -> String {
    if !s.starts_with('"') || !s.ends_with('"') {
        return s.to_string();
    }
    let s = &s[1..(s.len() - 1)];
    s.replace("\\n", "\n").replace("\\\"", "\"").replace("\\\\", "\\")
}
