use std::result::Result;

use crate::*;

use once_cell::sync::Lazy;

pub const DATA_STR: &str =
	include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/declarations/Permissions.csv"));

pub static DATA: Lazy<Permissions> = Lazy::new(|| {
	Permissions(
		csv::Reader::from_reader(DATA_STR.as_bytes())
			.deserialize()
			.collect::<Result<Vec<_>, _>>()
			.unwrap(),
	)
});

#[derive(Debug, Deserialize)]
pub struct EnumValue {
	pub name: String,
	pub doc: String,
}

#[derive(Default, Debug)]
pub struct Permissions(pub Vec<EnumValue>);
