use csv;
use ::*;

#[derive(Template)]
#[TemplatePath = "src/PermissionDeclarations.tt"]
#[derive(Default, Debug)]
pub struct Permissions(Vec<EnumValue>);

impl Declaration for Permissions {
    fn get_filename() -> &'static str { "Permissions.csv" }

    fn parse_from_read(read: &mut Read) -> Self {
        let mut table = csv::Reader::from_reader(read);
        Permissions(table.deserialize().collect::<Result<Vec<_>, _>>().unwrap())
    }
}