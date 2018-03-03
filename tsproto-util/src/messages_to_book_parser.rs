use ::*;
use message_parser::Message;
use book_parser::Struct;
use message_parser::Field;
use book_parser::Property;

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
    /// Unique access tuple to get the property
    op: RuleOp,
    id: Vec<String>,
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
        to: Vec<&'a Property>
    }
}

#[derive(Debug)]
enum RuleOp {
    Add,
    Remove,
    Update,
}

impl<'a> Declaration for MessagesToBookDeclarations<'a> {
    fn get_filename() -> &'static str { "MessagesToBook.txt" }

    fn parse(s: &str) -> Self {
        panic!()
    }
}