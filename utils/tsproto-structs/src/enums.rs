use once_cell::sync::Lazy;
use serde::Deserialize;

pub const DATA_STR: &str =
	include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/declarations/Enums.toml"));

pub static DATA: Lazy<Enums> = Lazy::new(|| toml::from_str(DATA_STR).unwrap());

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct Enums {
	#[serde(rename = "enum")]
	pub enums: Vec<Enum>,
	#[serde(rename = "bitflag")]
	pub bitflags: Vec<Enum>,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Enum {
	pub name: String,
	pub doc: Option<String>,
	#[serde(rename = "type")]
	pub use_type: Option<String>,
	pub variants: Vec<Variant>,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Variant {
	pub name: String,
	pub doc: String,
	pub value: Option<u64>,
}

impl Enum {
	/// Computes the needed number of bits to store all possible variants.
	pub fn bitwidth(&self) -> u32 {
		let mut leading_zeros = 0;
		let mut value: u64 = 0;
		for v in &self.variants {
			if let Some(v) = v.value {
				leading_zeros = std::cmp::min(leading_zeros, value.leading_zeros());
				value = v;
			} else {
				value += 1;
			}
		}
		leading_zeros = std::cmp::min(leading_zeros, value.leading_zeros());
		(64 - leading_zeros).next_power_of_two()
	}
}
