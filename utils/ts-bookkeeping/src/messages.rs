use std::net::IpAddr;

use thiserror::Error;
use time::{Duration, OffsetDateTime};
use tsproto_packets::commands::CommandParser;
use tsproto_packets::packets::{Direction, InHeader, OutCommand, PacketType};
use tsproto_types::errors::Error;

use crate::*;

type Result<T> = std::result::Result<T, ParseError>;

#[derive(Error, Debug)]
#[non_exhaustive]
pub enum ParseError {
	#[error(transparent)]
	Base64(#[from] base64::DecodeError),

	#[error("Parameter {arg} not found in {name}")]
	ParameterNotFound { arg: &'static str, name: &'static str },
	#[error("Parameter {arg} not found in {name}")]
	ParameterNotFound2 { arg: String, name: String },
	#[error("Command {0} is unknown")]
	UnknownCommand(String),
	#[error(transparent)]
	StringParse(#[from] std::str::Utf8Error),
	#[error(transparent)]
	TsProto(#[from] tsproto_packets::Error),
	/// Gets thrown when parsing a specific command with the wrong input.
	#[error("Command {0} is wrong")]
	WrongCommand(String),
	#[error("Wrong newprotocol flag ({0})")]
	WrongNewprotocol(bool),
	#[error("Wrong packet type {0:?}")]
	WrongPacketType(PacketType),
	#[error("Wrong direction {0:?}")]
	WrongDirection(Direction),
	#[error("Cannot parse \"{value}\" as int for parameter {arg} ({source})")]
	ParseInt { arg: &'static str, value: String, source: std::num::ParseIntError },
	#[error("Cannot parse \"{value}\" as SocketAddr for parameter {arg} ({source})")]
	ParseAddr { arg: &'static str, value: String, source: std::net::AddrParseError },
	#[error("Cannot parse \"{value}\" as float for parameter {arg} ({source})")]
	ParseFloat { arg: &'static str, value: String, source: std::num::ParseFloatError },
	#[error("Cannot parse \"{value}\" as bool for parameter {arg}")]
	ParseBool { arg: &'static str, value: String },
	#[error("Cannot parse \"{value}\" as SocketAddr for parameter {arg} ({source})")]
	ParseUid { arg: &'static str, value: String, source: base64::DecodeError },
	#[error("Cannot parse \"{value}\" as DateTimeOffset for parameter {arg} ({source})")]
	ParseDate { arg: &'static str, value: String, source: time::error::ComponentRange },
	#[error("Invalid value \"{value}\" for parameter {arg}")]
	InvalidValue { arg: &'static str, value: String },
}

pub trait InMessageTrait {
	fn new(header: &InHeader, args: CommandParser) -> Result<Self>
	where Self: Sized;
}

pub trait OutMessageTrait {
	fn to_packet(self) -> OutCommand;
}

pub trait OutMessageWithReturnTrait {
	fn to_packet(self, return_code: Option<&str>) -> OutCommand;
}

impl OutMessageTrait for OutCommand {
	fn to_packet(self) -> OutCommand { self }
}

pub mod s2c {
	use super::*;
	include!(concat!(env!("OUT_DIR"), "/s2c_messages.rs"));
}
pub mod c2s {
	use super::*;
	include!(concat!(env!("OUT_DIR"), "/c2s_messages.rs"));
}
