use std::fmt;

include!(concat!(env!("OUT_DIR"), "/versions.rs"));

impl fmt::Display for Version {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "{} {}", self.get_platform(), self.get_version_string())
	}
}
