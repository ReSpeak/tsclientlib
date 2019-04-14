use std::num::ParseFloatError;
use std::num::ParseIntError;

use chrono::naive::NaiveDateTime;
use chrono::{DateTime, Duration, Utc};
use failure::Fail;
use tsproto::packets::{Direction, PacketType};

use crate::errors::Error;
use crate::*;

#[derive(Fail, Debug)]
pub enum ParseError {
	#[fail(display = "Parameter {} not found in {}", arg, name)]
	ParameterNotFound {
		arg: &'static str,
		name: &'static str,
	},
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
		value, arg, error
	)]
	ParseInt {
		arg: &'static str,
		value: String,
		#[cause]
		error: ParseIntError,
	},
	#[fail(
		display = "Cannot parse \"{}\" as float for parameter {} ({})",
		value, arg, error
	)]
	ParseFloat {
		arg: &'static str,
		value: String,
		#[cause]
		error: ParseFloatError,
	},
	#[fail(
		display = "Cannot parse \"{}\" as bool for parameter {}",
		value, arg
	)]
	ParseBool { arg: &'static str, value: String },
	#[fail(display = "Invalid value \"{}\" for parameter {}", value, arg)]
	InvalidValue { arg: &'static str, value: String },
}

pub mod s2c {
	use super::*;
	include!(concat!(env!("OUT_DIR"), "/s2c_messages.rs"));
}
pub mod c2s {
	use super::*;
	include!(concat!(env!("OUT_DIR"), "/c2s_messages.rs"));
}
