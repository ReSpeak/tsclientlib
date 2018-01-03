
#[derive(Default, Debug)]
pub struct Error {
    pub name: String,
    pub doc: String,
    pub num: String,
}

pub(crate) fn parse(s: &str) -> ::Errors {
    ::Errors(s.lines().map(|l| {
        let mut fields = l.split(';');
        let res = Error {
            name: fields.next().unwrap().to_string(),
            doc: fields.next().unwrap().to_string(),
            num: fields.next().unwrap().to_string(),
        };
        assert!(fields.next().is_none());
        res
    }).collect())
}
