use ::*;

#[derive(Template)]
#[TemplatePath = "src/MessagesToBook.tt"]
#[derive(Default, Debug)]
pub struct MessagesToBook {
    book: BookDeclarations,
    messages: MessageDeclarations,
}

impl Declaration for MessagesToBook {
    fn get_filename() -> &'static str { "MessagesToBook.txt" }

    fn parse(s: &str) -> Self {
        panic!()
    }
}