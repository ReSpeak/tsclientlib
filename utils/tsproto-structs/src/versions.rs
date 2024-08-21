use std::collections::HashMap;
use std::fmt::Write;
use std::result::Result;

use crate::*;

use base64::prelude::*;
use once_cell::sync::Lazy;

pub const DATA_STR: &str =
	include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/declarations/Versions.csv"));

pub static DATA: Lazy<Versions> = Lazy::new(|| {
	let mut table = csv::Reader::from_reader(DATA_STR.as_bytes());
	let mut vs = Versions(table.deserialize().collect::<Result<Vec<_>, _>>().unwrap());

	// Add count if necessary
	let mut counts: HashMap<_, u32> = HashMap::new();
	for v in &vs.0 {
		let key = VersionKey::new(v);
		*counts.entry(key).or_default() += 1;
	}
	counts.retain(|_, c| *c > 1);

	for v in vs.0.iter_mut().rev() {
		let key = VersionKey::new(v);
		if let Some(count) = counts.get_mut(&key) {
			v.count = *count;
			*count -= 1;
		}
	}

	vs
});

#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct Version {
	pub version: String,
	pub platform: String,
	pub hash: String,
	#[serde(default)]
	count: u32,
}

impl Version {
	pub fn get_enum_name(&self) -> String {
		let mut res = String::new();
		res.push_str(&self.platform.replace([' ', '.'], "_"));
		let ver = self.version.split(' ').next().unwrap().replace('-', "_");
		for num in ver.split('.') {
			res.push('_');
			if num != "?" {
				res.push_str(num);
			} else {
				res.push('X');
			}
		}
		if self.count != 0 {
			res.push_str("__");
			res.push_str(&self.count.to_string());
		}
		res
	}

	pub fn get_sign_array(&self) -> String {
		let mut res = String::new();
		for b in BASE64_STANDARD.decode(&self.hash).unwrap() {
			if !res.is_empty() {
				res.push_str(", ");
			}
			let _ = write!(res, "{:#x}", b);
		}
		res
	}
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct VersionKey {
	pub version: String,
	pub platform: String,
}

impl VersionKey {
	fn new(v: &Version) -> Self {
		Self {
			version: v.version.split(' ').next().unwrap().to_string(),
			platform: v.platform.clone(),
		}
	}
}

#[derive(Default, Debug)]
pub struct Versions(pub Vec<Version>);
