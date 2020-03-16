use std::net::{AddrParseError, IpAddr};
use std::num::ParseFloatError;
use std::num::ParseIntError;
use std::str::Utf8Error;

use failure::Fail;
use time::{Duration, OffsetDateTime};
use slog::Logger;
use tsproto_packets::commands::CommandParser;
use tsproto_packets::packets::{Direction, InHeader, PacketType};
use tsproto_types::errors::Error;

use crate::*;

type Result<T> = std::result::Result<T, ParseError>;

#[derive(Fail, Debug)]
#[non_exhaustive]
pub enum ParseError {
	#[fail(display = "Parameter {} not found in {}", arg, name)]
	ParameterNotFound { arg: &'static str, name: &'static str },
	#[fail(display = "Parameter {} not found in {}", arg, name)]
	ParameterNotFound2 { arg: String, name: String },
	#[fail(display = "Command {} is unknown", _0)]
	UnknownCommand(String),
	#[fail(display = "{}", _0)]
	StringParse(Utf8Error),
	#[fail(display = "{}", _0)]
	TsProto(tsproto_packets::Error),
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
		display = "Cannot parse \"{}\" as SocketAddr for parameter {} ({})",
		value, arg, error
	)]
	ParseAddr {
		arg: &'static str,
		value: String,
		#[cause]
		error: AddrParseError,
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
	#[fail(
		display = "Cannot parse \"{}\" as SocketAddr for parameter {} ({})",
		value, arg, error
	)]
	ParseUid {
		arg: &'static str,
		value: String,
		#[cause]
		error: base64::DecodeError,
	},
	#[fail(display = "Invalid value \"{}\" for parameter {}", value, arg)]
	InvalidValue { arg: &'static str, value: String },
}

impl From<Utf8Error> for ParseError {
	fn from(e: Utf8Error) -> Self { ParseError::StringParse(e) }
}
impl From<tsproto_packets::Error> for ParseError {
	fn from(e: tsproto_packets::Error) -> Self { ParseError::TsProto(e) }
}

pub trait InMessageTrait {
	fn new(logger: &Logger, header: &InHeader, args: CommandParser) -> Result<Self> where Self: Sized;
}

pub mod s2c {
	use super::*;
	include!(concat!(env!("OUT_DIR"), "/s2c_messages.rs"));
}
pub mod c2s {
	use super::*;
	include!(concat!(env!("OUT_DIR"), "/c2s_messages.rs"));
}
