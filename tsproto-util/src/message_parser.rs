use ::*;

#[derive(Template)]
#[TemplatePath = "src/MessageDeclarations.tt"]
#[derive(Deserialize, Debug, Clone)]
pub struct MessageDeclarations {
    pub(crate) fields: Vec<Field>,
    pub(crate) msg_group: Vec<MessageGroup>,
}

impl Declaration for MessageDeclarations {
    type Dep = ();

    fn get_filename() -> &'static str { "Messages.toml" }

    fn parse(s: &str, (): Self::Dep) -> MessageDeclarations {
        ::toml::from_str(s).unwrap()
    }
}

impl MessageDeclarations {
    pub fn get_message(&self, name: &str) -> &Message {
        if let Some(m) = self.msg_group.iter().flat_map(|g| g.msg.iter()).find(|m| m.name == name) {
            m
        } else {
            panic!("Cannot find message {}", name);
        }
    }
    pub fn get_field(&self, mut map: &str) -> &Field {
        if map.ends_with('?') {
            map = &map[..map.len() - 1];
        }
        if let Some(f) = self.fields.iter().find(|f| f.map == map) {
            f
        } else {
            panic!("Cannot find field {}", map);
        }
    }
}

#[derive(Deserialize, Debug, Clone, Eq, PartialEq)]
pub struct Field {
    /// Internal name of this declarations file to map fields to messages.
    pub map: String,
    /// The name as called by TeamSpeak in messages.
    pub ts: String,
    /// The pretty name in PascalCase. This will be used for the fields in rust.
    pub pretty: String,
    #[serde(rename = "type")]
    pub type_s: String,
    #[serde(rename = "mod")]
    pub modifier: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct MessageGroup {
    pub default: MessageGroupDefaults,
    pub msg: Vec<Message>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct MessageGroupDefaults {
    pub s2c: bool,
    pub c2s: bool,
    pub response: bool,
    pub low: bool,
    pub np: bool,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Message {
    /// How we call this message.
    pub name: String,
    /// How TeamSpeak calls this message.
    pub notify: Option<String>,
    pub attributes: Vec<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct MessageField {
    pub mapping_name: String,
    pub optional: bool,
}

impl Field {
    pub fn get_rust_name(&self) -> String {
        to_snake_case(&self.pretty)
    }
    /// Takes the attribute to look if it is optional
    pub fn get_rust_type(&self, a: &str) -> String {
        let mut res = convert_type(&self.type_s);

        if self.modifier.as_ref().map(|s| s == "array").unwrap_or(false) {
            res = format!("Vec<{}>", res);
        }
        if a.ends_with('?') {
            res = format!("Option<{}>", res);
        }
        res
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
    } else if t.starts_with("Duration") {
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
