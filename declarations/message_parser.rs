use ::Declarations;

#[derive(Default, Debug)]
pub struct Field {
    pub ts_name: String,
    pub name: String,
    pub type_s: String,
}

#[derive(Default, Debug)]
pub struct Message {
    pub class_name: String,
    pub notify_name: String,
    pub params: Vec<String>,
}

#[derive(Default, Debug)]
pub struct Notify {
    pub enum_name: String,
}

pub(crate) fn parse(s: &str) -> Declarations {
    let mut decls = Declarations::default();

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
                    notify_name: params[1].to_string(),
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
                    type_s: convert_type(params[3]),
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
            "CONV" => {
                if params.len() < 2 {
                    panic!("Invalid CONV: {}", l);
                }
            }
            "BREAK" => {
                break;
            }
            _ => panic!("Unknown type: '{}'", type_s),
        }
    }

    decls
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
    } else if t == "ClientUid" {
        String::from("Vec<u8>")
    } else if t == "Ts3ErrorCode" {
        String::from("Error")
    } else {
        t.into()
    }
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
