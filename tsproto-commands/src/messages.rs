use std::num::ParseFloatError;
use std::num::ParseIntError;

use errors::Error;
use *;

pub trait Response {
	fn get_return_code(&self) -> Option<&str>;
	fn set_return_code(&mut self, return_code: Option<String>);
}

pub trait TryParse<T>: Sized {
	type Err;
	fn try_from(T) -> Result<Self, Self::Err>;
}

#[derive(Fail, Debug)]
pub enum ParseError {
	#[fail(display = "Parameter {} not found in {}", arg, name)]
	ParameterNotFound { arg: &'static str, name: &'static str },
	#[fail(display = "Command {} is unknown", _0)]
	UnknownCommand(String),
	/// Gets thrown when parsing a specific command with the wrong input.
	#[fail(display = "Command {} is wrong", _0)]
	WrongCommand(String),
	#[fail(display = "Wrong newprotocol flag ({})", _0)]
	WrongNewprotocol(bool),
	#[fail(display = "Wrong packet type {:?}", _0)]
	WrongPacketType(PacketType),
	#[fail(display = "Wrong direction {:?}", _0)]
	WrongDirection(Direction),
	#[fail(
		display = "Cannot parse \"{}\" as int for parameter {} ({})",
		value,
		arg,
		error
	)]
	ParseInt {
		arg: &'static str,
		value: String,
		#[cause]
		error: ParseIntError,
	},
	#[fail(
		display = "Cannot parse \"{}\" as float for parameter {} ({})",
		value,
		arg,
		error
	)]
	ParseFloat {
		arg: &'static str,
		value: String,
		#[cause]
		error: ParseFloatError,
	},
	#[fail(
		display = "Cannot parse \"{}\" as bool for parameter {}",
		value,
		arg
	)]
	ParseBool { arg: &'static str, value: String },
	#[fail(
		display = "Invalid value \"{}\" for parameter {}",
		value,
		arg
	)]
	InvalidValue { arg: &'static str, value: String },
}

include!(concat!(env!("OUT_DIR"), "/messages.rs"));
