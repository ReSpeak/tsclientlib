use csv;
use std::io::Read;
use ::*;

#[derive(Template)]
#[TemplatePath = "src/ErrorDeclarations.tt"]
#[derive(Default, Debug)]
pub struct Errors(Vec<EnumValue>);

impl Declaration for Errors {
    type Dep = ();

    fn get_filename() -> &'static str { "Errors.csv" }

    fn parse_from_read(read: &mut Read, (): Self::Dep) -> Self {
        let mut table = csv::Reader::from_reader(read);
        Errors(table.deserialize().collect::<Result<Vec<_>, _>>().unwrap())
    }
}