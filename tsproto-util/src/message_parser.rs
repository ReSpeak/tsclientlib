use regex::{CaptureMatches, Regex};
use ::*;

#[derive(Template)]
#[TemplatePath = "src/MessageDeclarations.tt"]
#[derive(Default, Debug)]
pub struct MessageDeclarations {
    pub(crate) fields: Map<String, Field>,
    pub(crate) messages: Map<String, Message>,
    pub(crate) notifies: Vec<String>,
}

impl Declaration for MessageDeclarations {
    type Dep = ();

    fn get_filename() -> &'static str { "Messages.txt" }

    fn parse(s: &str, (): Self::Dep) -> MessageDeclarations {
        const ATTRIB_LIST: &'static str = r#"(\s*(?P<pname>\w+):(?P<pval>\w+|"[^"]+"|\[[\w\s]+\]))*"#;
        let param_re = Regex::new(&format!{"^{}$", ATTRIB_LIST}).unwrap();
        let msg_re = Regex::new(&format!{r#"^\s*(?P<msg>\w+)\s*(;(?P<ps>{})(\s*;\s*(?P<flds>[\w\s\?,]*))?)?\s*$"#, ATTRIB_LIST}).unwrap();

        let mut decls = MessageDeclarations::default();
        let mut default_vals = Values::default();

        for l in s.lines() {
            if l.trim().is_empty() || l.trim().starts_with("#") {
                continue;
            }

            let parts: Vec<_> = l.splitn(2, ':').collect();
            if parts.len() < 2 || parts[0].trim().is_empty() {
                continue;
            }
            let type_s = parts[0].trim();
            match type_s.to_uppercase().as_str() {
                "MSG" => {
                    let captures = msg_re.captures(parts[1]).expect(&format!("Invalid MSG: {}", l));
                    let msg_name = captures["msg"].to_string();

                    let params = if let Some(flds) = captures.name("flds") {
                        flds.as_str().split(',').map(|p| p.trim()).filter(|p| !p.is_empty()).map(|p| MessageField {
                        mapping_name: p.trim_right_matches("?").to_string(),
                        optional: p.ends_with("?") }).collect()
                    } else {
                        Vec::<_>::new()
                    };

                    let mut values = default_vals.clone();
                    let captures_ps = param_re.captures_iter(&captures["ps"]);
                    values.fill(captures_ps, l);
                    values.response = Some(false); // TODO remove this hack

                    if values.notify.is_some() {
                        decls.notifies.push(msg_name.clone());
                    }

                    decls.messages.insert(msg_name.clone(), Message {
                        class_name: msg_name.clone(),
                        values,
                        params,
                    });
                }
                "FIELD" => {
                    let stripped = parts[1].replace(' ', "");
                    let mut params: Vec<_> = stripped.split(',').collect();
                    if params.len() < 4 {
                        panic!("Invalid FIELD: {}", l);
                    }
                    decls.fields.insert(params[0].to_string(), Field {
                        ts_name: params[1].to_string(),
                        name: params[2].to_string(),
                        rust_name: ::to_snake_case(params[2]),
                        rust_type: convert_type(params[3]),
                        type_orig: params[3].to_string(),
                    });
                }
                "TYPE" => {}
                "DEFAULT" => {
                    let captures = param_re.captures_iter(parts[1]);
                    default_vals = Values::default();
                    default_vals.fill(captures, l);
                }
                "BREAK" => {
                    break;
                }
                _ => panic!("Unknown type: '{}'", type_s),
            }
        }

        decls
    }
}

#[derive(Default, Debug, Eq, PartialEq)]
pub struct Field {
    pub ts_name: String,
    pub name: String,
    /// rust callable name in snake_case
    pub rust_name: String,
    pub type_orig: String,
    pub rust_type: String,
}

#[derive(Default, Debug)]
pub struct Message {
    pub class_name: String,
    pub values: Values,
    /// list of: mapping name of a field used by this message.
    pub params: Vec<MessageField>,
}

impl Message {
    pub fn is_notify(&self) -> bool { self.values.notify.is_some() }
    pub fn get_notify_name(&self) -> bool { self.values.notify.is_some() }
    pub fn is_response(&self) -> bool { self.values.response.expect("'response' property is undefined.") }
    pub fn is_s2c(&self) -> bool { self.values.response.expect("'s2c' property is undefined.") }
    pub fn is_c2s(&self) -> bool { self.values.response.expect("'c2s' property is undefined.") }
    pub fn is_low(&self) -> bool { self.values.response.expect("'low' property is undefined.") }
    pub fn is_np(&self) -> bool { self.values.response.expect("'np' property is undefined.") }
}

#[derive(Default, Debug)]
pub struct MessageField {
    pub mapping_name: String,
    pub optional: bool,
}

#[derive(Default, Clone, Debug)]
pub struct Values {
    pub notify: Option<String>,
    pub response: Option<bool>,
    pub s2c: Option<bool>,
    pub c2s: Option<bool>,
    pub low: Option<bool>,
    pub np: Option<bool>,
}

impl Values {
    pub fn fill<'r, 't>(&mut self, captures: CaptureMatches<'r, 't>, l: &str) {
        for cap in captures {
            let key = if let Some(key_mat) = cap.name("pname") {
                key_mat.as_str().to_lowercase()
            } else {
                return;
            };
            let val = cap["pval"].trim();
            match key.as_str() {
                "notify" => self.notify = Some(unquote(val)),
                "s2c" => self.s2c = Some(val.parse().unwrap()),
                "c2s" => self.c2s = Some(val.parse().unwrap()),
                "response" => self.response = Some(val.parse().unwrap()),
                "low" => self.low = Some(val.parse().unwrap()),
                "np" => self.np = Some(val.parse().unwrap()),
                _ => { panic!("Invalid value '{}' in line {}", key, l); }
            }
        }
    }
}

pub fn convert_type(t: &str) -> String {
    if t.ends_with("[]") {
        let inner = &t[..(t.len() - 2)];
        return format!("Vec<{}>", convert_type(inner));
    }
    if t.ends_with("T") {
        return convert_type(&t[..(t.len() - 1)]);
    }

    if t == "str" || t == "string" {
        String::from("String")
    } else if t == "byte" {
        String::from("u8")
    } else if t == "ushort" {
        String::from("u16")
    } else if t == "int" {
        String::from("i32")
    } else if t == "uint" {
        String::from("u32")
    } else if t == "float" {
        String::from("f32")
    } else if t == "long" {
        String::from("i64")
    } else if t == "ulong" {
        String::from("u64")
    } else if t == "ushort" {
        String::from("u16")
    } else if t == "DateTime" {
        String::from("DateTime<Utc>")
    } else if t.starts_with("TimeSpan") {
        String::from("Duration")
    } else if t == "ClientUid" || t == "ClientUidT" {
        String::from("Uid")
    } else if t == "Ts3ErrorCode" {
        String::from("Error")
    } else if t == "PermissionId" {
        String::from("Permission")
    } else {
        t.into()
    }
}
