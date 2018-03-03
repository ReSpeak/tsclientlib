use ::*;

#[derive(Template)]
#[TemplatePath = "src/MessageDeclarations.tt"]
#[derive(Default, Debug)]
pub struct MessageDeclarations {
    pub(crate) fields: Map<String, Field>,
    pub(crate) messages: Map<String, Message>,
    notifies: Map<String, Notify>,
}

impl Declaration for MessageDeclarations {
    type Dep = ();

    fn get_filename() -> &'static str { "Messages.txt" }

    fn parse(s: &str, (): Self::Dep) -> MessageDeclarations {
        let mut decls = MessageDeclarations::default();

        for l in s.lines() {
            if l.trim().is_empty() {
                continue;
            }

            let parts: Vec<_> = l.splitn(2, ':').collect();
            if parts.len() < 2 || parts[0].trim().is_empty() {
                continue;
            }
            let type_s = parts[0].trim();
            let stripped = parts[1].replace(' ', "");
            let mut params: Vec<_> = stripped.split(',').collect();
            match type_s.to_uppercase().as_str() {
                "MSG" => {
                    if params.len() < 2 {
                        panic!("Invalid MSG: {}", l);
                    }
                    decls.messages.insert(params[0].to_string(), Message {
                        class_name: params[0].to_string(),
                        notify_name: params[1].trim_left_matches('+').to_string(),
                        is_notify: !params[1].is_empty(),
                        is_response: (params[1].is_empty() || params[1].starts_with("+")) && false,
                        params: params.drain(2..).map(|p| p.to_string()).collect(),
                    });
                }
                "FIELD" => {
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
                "NOTIFY" => {
                    if params.len() < 2 {
                        panic!("Invalid NOTIFY: {}", l);
                    }
                    decls.notifies.insert(params[0].to_string(), Notify {
                        enum_name: params[1].to_string(),
                    });
                }
                "TYPE" => {}
                "BREAK" => {
                    break;
                }
                _ => panic!("Unknown type: '{}'", type_s),
            }
        }

        decls
    }
}

#[derive(Default, Debug)]
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
    pub notify_name: String,
    pub is_notify: bool,
    pub is_response: bool,
    /// list of: mapping name of a field used by this message.
    pub params: Vec<String>,
}

#[derive(Default, Debug)]
pub struct Notify {
    pub enum_name: String,
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
