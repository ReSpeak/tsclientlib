use std::net::{AddrParseError, IpAddr};
use std::num::ParseFloatError;
use std::num::ParseIntError;

use chrono::naive::NaiveDateTime;
use chrono::{DateTime, Duration, Utc};
use failure::Fail;
use tsproto_packets::packets::{Direction, PacketType};
use tsproto_packets::commands::CanonicalCommand;
use tsproto_types::errors::Error;

use crate::*;

type Result<T> = std::result::Result<T, ParseError>;

#[derive(Fail, Debug)]
pub enum ParseError {
	#[fail(display = "Parameter {} not found in {}", arg, name)]
	ParameterNotFound {
		arg: &'static str,
		name: &'static str,
	},
	#[fail(display = "Parameter {} not found in {}", arg, name)]
	ParameterNotFound2 {
		arg: String,
		name: String,
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
	#[fail(display = "Invalid value \"{}\" for parameter {}", value, arg)]
	InvalidValue { arg: &'static str, value: String },
}

pub trait CommandExt {
	fn get_invoker(&self) -> Result<Option<Invoker>>;
	fn get_arg(&self, name: &str) -> Result<&str>;
}

impl CommandExt for CanonicalCommand<'_> {
	fn get_invoker(&self) -> Result<Option<Invoker>> {
		if let Some(id) = self.get("invokerid") {
			if let Some(name) = self.get("invokername") {
				Ok(Some(Invoker {
					id: ClientId(id.parse().map_err(|e| ParseError::ParseInt {
						arg: "invokerid",
						value: id.into(),
						error: e,
					})?),
					name: name.to_string(),
					uid: self.get("invokeruid").map(|i| Uid(i.to_string())),
				}))
			} else {
				Ok(None)
			}
		} else {
			Ok(None)
		}
	}

	fn get_arg(&self, name: &str) -> Result<&str> {
		self.get(name)
			.ok_or_else(|| ParseError::ParameterNotFound2 {
				arg: name.into(),
				name: "unknown".into(),
			})
	}
}

pub mod s2c {
	use super::*;
	include!(concat!(env!("OUT_DIR"), "/s2c_messages.rs"));
}
pub mod c2s {
	use super::*;
	include!(concat!(env!("OUT_DIR"), "/c2s_messages.rs"));
}
