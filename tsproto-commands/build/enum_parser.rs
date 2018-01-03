#[derive(Default, Debug)]
pub struct EnumValue {
    pub name: String,
    pub doc: String,
    pub num: String,
}

pub(crate) fn parse(s: &str) -> Vec<EnumValue> {
    s.lines().map(|l| {
        let mut fields = l.split(';');
        let res = EnumValue {
            name: fields.next().unwrap().to_string(),
            doc: fields.next().unwrap().to_string(),
            num: fields.next().unwrap().to_string(),
        };
        assert!(fields.next().is_none());
        res
    }).collect()
}
