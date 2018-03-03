use csv;
use std::io::Read;
use ::*;

#[derive(Template)]
#[TemplatePath = "src/ErrorDeclarations.tt"]
#[derive(Default, Debug)]
pub struct Errors(Vec<EnumValue>);

impl Declaration for Errors {
    fn get_filename() -> &'static str { "Errors.csv" }

    fn parse_from_read(read: &mut Read) -> Self {
        let mut table = csv::Reader::from_reader(read);
        Errors(table.deserialize().collect::<Result<Vec<_>, _>>().unwrap())
    }
}