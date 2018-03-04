use ::*;
use message_parser::{Message, Field};
use book_parser::{Struct, Property, Nested};
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
        to: Vec<PropKind<'a>>
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

#[derive(Debug)]
enum PropKind<'a> {
    Prop(&'a Property),
    Nested(&'a Nested),
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

                let set_msg = messages.messages.get(msg).expect(&format!("Cannot find message defined in line {}", i));
                let msg_fields = set_msg.params.iter().map(|f| messages.fields.get(f)).map(Option::unwrap).collect::<Vec<_>>();
                let set_stru = book.structs.iter().find(|s| s.name == stru).expect(&format!("Cannot find struct defined in line {}", i));
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
                let buildev = buildev.as_mut().expect(&format!("No event declaration found before this rule (line {})", i));

                let capture = rgx_rule.captures(&trimmed).unwrap();

                let find_prop = |name, book_struct: &Struct| {
                    if let Some(prop) = book.properties.iter().find(|p| p.struct_name == book_struct.name && p.name == name) {
                        return PropKind::Prop(prop);
                    }
                    if let Some(nest) = book.nesteds.iter().find(|p| p.struct_name == book_struct.name && p.name == name) {
                        return PropKind::Nested(nest);
                    }
                    panic!("No such (nested) property found in struct (line {})", i);
                };

                let fld = &capture["fld"];
                let prop = &capture["prop"];
                let modi = &capture["mod"];

                let set_modi = mod_to_enu(modi, i);

                let event = if prop.starts_with("(") { // Function
                    RuleKind::Function {
                        name: fld.to_string(),
                        to: prop.trim_left_matches('(').trim_right_matches(')').split(",").map(|x| x.trim()).map(|x| find_prop(x, buildev.0.book_struct)).collect::<Vec<_>>(),
                    }
                } else { // Map
                    if let PropKind::Prop(prop) = find_prop(prop, buildev.0.book_struct) {
                        let from_fld = find_field(fld, &buildev.1, i);
                        if from_fld.name == prop.name {
                            println!("This field assignment is already implicitly defined (line {})", i);
                        }
                        RuleKind::Map {
                            from: from_fld,
                            to: prop,
                            op: set_modi
                        }
                    }
                    else { panic!("Mapped values cannot be nested types.") }
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
                    for propk in to {
                        match *propk {
                            PropKind::Prop(prop) => used_props.push(prop),
                            _ => {}
                        }
                    }
                },
                _ => {}
            }
        }

        for fld in flds {
            if used_flds.contains(&fld) {
                continue;
            }
            if let Some(prop) = book.properties.iter().find(|p| p.struct_name == ev.book_struct.name && p.name == fld.name) {
                if used_props.contains(&prop) {
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
    let name_snake = to_snake_case(name);
    *msg_fields.iter().find(|f| f.rust_name == name_snake).expect(&format!("Cannot find field '{}' in line {}", name, i))
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

fn get_prop_name<'a>(p: &PropKind<'a>) -> String {
    to_snake_case(match *p {
        PropKind::Prop(ref p) => &p.name,
        PropKind::Nested(ref p) => &p.name,
    })
}

fn get_id_args(event: &Event) -> String {
    let mut res = String::new();
    for id in &event.id {
        if !res.is_empty() {
            res.push_str(", ");
        }
        if let IdKind::Fld(f) = *id {
            if is_ref_type(&f.rust_type) {
                res.push('&');
            }
            res.push_str("cmd.");
            res.push_str(&f.rust_name);
        } else {
            res.push_str("self.id");
        }
    }
    res
}

fn gen_return_match(to: &[PropKind]) -> String {
    if to.len() == 1 {
        get_prop_name(&to[0])
    } else {
        format!("({})", join(to.iter().map(get_prop_name), ", "))
    }
}