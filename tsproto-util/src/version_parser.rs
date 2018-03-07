use csv;
use ::*;

#[derive(Debug, Deserialize)]
pub struct Version {
    pub version: String,
    pub platform: String,
    pub hash: String,
}

impl Version {
    fn get_enum_name(&self) -> String {
        let mut res = String::new();
        res.push_str(&self.platform.replace(' ', "_"));
        let ver = self.version.split(' ').next().unwrap();
        for num in ver.split('.') {
            if num != "?" {
                res.push('_');
                res.push_str(&format!("{}", num));
            }
        }
        res
    }

    fn get_sign_array(&self) -> String {
        let mut res = String::new();
        for b in ::base64::decode(&self.hash).unwrap() {
            if !res.is_empty() {
                res.push_str(", ");
            }
            res.push_str(&format!("{:#x}", b));
        }
        res
    }
}

#[derive(Template)]
#[TemplatePath = "src/VersionDeclarations.tt"]
#[derive(Default, Debug)]
pub struct Versions(Vec<Version>);

impl Declaration for Versions {
    type Dep = ();

    fn get_filename() -> &'static str { "Versions.csv" }

    fn parse_from_read(read: &mut Read, (): Self::Dep) -> Self {
        let mut table = csv::Reader::from_reader(read);
        Versions(table.deserialize().collect::<Result<Vec<_>, _>>().unwrap())
    }
}
