use std::result::Result;

use crate::*;

use once_cell::sync::Lazy;

pub const DATA_STR: &str =
	include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/declarations/Errors.csv"));

pub static DATA: Lazy<Errors> = Lazy::new(|| {
	Errors(
		csv::Reader::from_reader(DATA_STR.as_bytes())
			.deserialize()
			.collect::<Result<Vec<_>, _>>()
			.unwrap(),
	)
});

#[derive(Default, Debug)]
pub struct Errors(pub Vec<EnumValue>);
