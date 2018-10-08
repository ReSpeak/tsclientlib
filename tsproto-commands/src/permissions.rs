use std::fmt;

include!(concat!(env!("OUT_DIR"), "/permissions.rs"));

impl fmt::Display for Permission {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		fmt::Debug::fmt(self, f)
	}
}
