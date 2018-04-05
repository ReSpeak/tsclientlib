use ::*;
use message_parser::{Message, Field};
use book_parser::{Struct, Property, PropId};
use regex::{Regex};
use std::io::BufReader;

#[derive(Template)]
#[TemplatePath = "src/MessagesToBook.tt"]
#[derive(Debug)]
pub struct MessagesToBookDeclarations<'a> {
    book: &'a BookDeclarations,
    messages: &'a MessageDeclarations,
    decls: Vec<Event<'a>>,
}

#[derive(Debug)]
struct Event<'a> {
    op: RuleOp,
    /// Unique access tuple to get the property
    id: Vec<IdKind<'a>>,
    msg: &'a Message,
    book_struct: &'a Struct,
    rules: Vec<RuleKind<'a>>,
}

#[derive(Debug)]
enum RuleKind<'a> {
    Map {
        from: &'a Field,
        to: &'a Property,
        op: RuleOp,
    },
    Function {
        name: String,
        to: Vec<&'a Property>,
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum RuleOp {
    Add,
    Remove,
    Update,
}

#[derive(Debug)]
enum IdKind<'a> {
    Fld(&'a Field),
    Id,
}

impl<'a> Declaration for MessagesToBookDeclarations<'a> {
    type Dep = (&'a BookDeclarations, &'a MessageDeclarations);

    fn get_filename() -> &'static str { "MessagesToBook.txt" }

    fn parse_from_read(read: &mut Read, (book, messages): Self::Dep) -> Self {
        let rgx_event = Regex::new(r##"^\s*(?P<msg>\w+)\s*->\s*(?P<stru>\w+)\[(?P<id>(\s*(@|\w+))*)\](?P<mod>(\+|-)?)$"##).unwrap();
        let rgx_rule = Regex::new(r##"^\s*(?P<fld>\w+)\s*->\s*(?P<prop>(\w+|\(((\s*\w+\s*)(,(\s*\w+\s*))*)\)))(?P<mod>(\+|-)?)$"##).unwrap();

        let mut decls = Vec::new();
        let mut buildev: Option<(Event, Vec<&Field>)> = None;

        for (i, line) in BufReader::new(read).lines().map(Result::unwrap).enumerate() {
            let i = i + 1;
            let trimmed = line.trim_left();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            if rgx_event.is_match(&trimmed) {
                finalize(book, &mut decls, buildev);

                let capture = rgx_event.captures(&trimmed).unwrap();

                let msg = &capture["msg"];
                let stru = &capture["stru"];
                let id = &capture["id"];
                let modi = &capture["mod"];

                let set_msg = messages.get_message(msg);
                let msg_fields = set_msg.attributes.iter()
                    .map(|a| messages.get_field(a)).collect::<Vec<_>>();
                let set_stru = book.structs.iter().find(|s| s.name == stru)
                    .expect(&format!("Cannot find struct {} defined in line {}",
                        stru, i));
                let set_id = id
                    .split(" ")
                    .map(|s| s.trim())
                    .map(|s|
                        if s.starts_with('@') { IdKind::Id }
                        else { IdKind::Fld(find_field(s, &msg_fields, i)) } )
                    .collect::<Vec<_>>();
                let set_modi = mod_to_enu(modi, i);

                buildev = Some((Event {
                    op: set_modi,
                    id: set_id,
                    msg: set_msg,
                    book_struct: set_stru,
                    rules: Vec::new(),
                }, msg_fields));
            } else if rgx_rule.is_match(&trimmed) {
                let buildev = buildev.as_mut()
                    .expect(&format!("No event declaration found before this \
                        rule (line {})", i));

                let capture = rgx_rule.captures(&trimmed).unwrap();

                let find_prop = |name, book_struct: &'a Struct| -> &'a Property {
                    if let Some(prop) = book_struct.properties.iter()
                        .find(|p| p.name == name) {
                        return prop;
                    }
                    panic!("No such (nested) property {} found in struct (line {})", name, i);
                };

                let fld = &capture["fld"];
                let prop = &capture["prop"];
                let modi = &capture["mod"];

                let set_modi = mod_to_enu(modi, i);

                let event = if prop.starts_with("(") { // Function
                    RuleKind::Function {
                        name: fld.to_string(),
                        to: prop.trim_left_matches('(').trim_right_matches(')')
                            .split(",")
                            .map(|x| x.trim())
                            .map(|x| find_prop(x, buildev.0.book_struct))
                            .collect::<Vec<_>>(),
                    }
                } else { // Map
                    let prop = find_prop(prop, buildev.0.book_struct);
                    let from_fld = find_field(fld, &buildev.1, i);
                    if from_fld.pretty == prop.name {
                        println!("This field assignment is already implicitly defined (line {})", i);
                    }
                    RuleKind::Map {
                        from: from_fld,
                        to: prop,
                        op: set_modi
                    }
                };
                buildev.0.rules.push(event);
            } else {
                panic!("Parse error in line {}", i);
            }
        }

        finalize(book, &mut decls, buildev);

        MessagesToBookDeclarations{ book, messages, decls }
    }
}

fn finalize<'a>(book: &'a BookDeclarations, decls: &mut Vec<Event<'a>>, buildev: Option<(Event<'a>, Vec<&'a Field>)>) {
    if let Some((mut ev, flds)) = buildev {
        let used_flds = ev.rules
            .iter()
            .filter_map(|f| match *f { RuleKind::Map{ from, .. } => Some(from), _ => None, })
            .collect::<Vec<_>>();

        let mut used_props = vec![];
        for rule in &ev.rules {
            match rule {
                &RuleKind::Function{ ref to, .. } => {
                    for p in to {
                        used_props.push(p.name.clone());
                    }
                },
                _ => {}
            }
        }

        for fld in flds {
            if used_flds.contains(&fld) {
                continue;
            }
            if let Some(prop) = book.get_struct(&ev.book_struct.name).properties
                .iter().find(|p| p.name == fld.pretty) {
                if used_props.contains(&prop.name) {
                    continue;
                }

                ev.rules.push(RuleKind::Map {
                    from: fld,
                    to: prop,
                    op: RuleOp::Update,
                });
            }
        }

        decls.push(ev);
    }
}

// the in rust callable name (in PascalCase) from the field
fn find_field<'a>(name: &str, msg_fields: &Vec<&'a Field>, i: usize) -> &'a Field {
    *msg_fields.iter().find(|f| f.pretty == name)
        .expect(&format!("Cannot find field '{}' in line {}", name, i))
}

fn mod_to_enu(modi: &str, i: usize) -> RuleOp {
    match modi {
        "+" => RuleOp::Add,
        "-" => RuleOp::Remove,
        "" => RuleOp::Update,
        _ => panic!("Unknown modifier in line {}", i)
    }
}

impl<'a> RuleKind<'a> {
    fn is_function(&self) -> bool {
        if let RuleKind::Function { .. } = *self {
            true
        } else {
            false
        }
    }
}

fn get_id_args(event: &Event) -> String {
    let mut res = String::new();
    for id in &event.id {
        if !res.is_empty() {
            res.push_str(", ");
        }
        if let IdKind::Fld(f) = *id {
            if is_ref_type(&f.get_rust_type("")) {
                res.push('&');
            }
            res.push_str("cmd.");
            res.push_str(&f.get_rust_name());
        }
    }
    res
}

fn get_notification_field(from: &Field) -> String {
    let rust_type = from.get_rust_type("");
    if rust_type == "String" || rust_type == "Uid" || rust_type.starts_with("Vec<") {
        format!("{}.clone()", from.get_rust_name())
    } else {
        from.get_rust_name().clone()
    }
}

fn gen_return_match(to: &[&Property]) -> String {
    if to.len() == 1 {
        to_snake_case(&to[0].name)
    } else {
        format!("({})", join(to.iter().map(|p| to_snake_case(&p.name)), ", "))
    }
}

fn try_result(s: &str) -> &'static str {
    match s {
        "get_mut_client" |
        "get_mut_channel" |
        "add_connection_client_data" => "?",
        _ => "",
    }
}
