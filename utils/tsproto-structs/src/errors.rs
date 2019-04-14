use crate::*;
use lazy_static::lazy_static;

pub const DATA_STR: &str = include_str!(concat!(
	env!("CARGO_MANIFEST_DIR"),
	"/declarations/Errors.csv"
));

lazy_static! {
	pub static ref DATA: Errors = Errors(
		csv::Reader::from_reader(DATA_STR.as_bytes())
			.deserialize()
			.collect::<Result<Vec<_>, _>>()
			.unwrap()
	);
}

#[derive(Default, Debug)]
pub struct Errors(pub Vec<EnumValue>);
