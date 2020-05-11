use std::fmt;

use num_derive::{FromPrimitive, ToPrimitive};

include!(concat!(env!("OUT_DIR"), "/errors.rs"));

impl fmt::Display for Error {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result { fmt::Debug::fmt(self, f) }
}

impl std::error::Error for Error {
	fn description(&self) -> &str { "TeamSpeak error" }
}
