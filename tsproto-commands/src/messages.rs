use std::num::ParseIntError;
use std::num::ParseFloatError;

use ::*;
use errors::Error;
use permissions::Permission;

pub trait Response {
    fn get_return_code(&self) -> &str;
    fn set_return_code(&mut self, return_code: String);
}

pub trait TryParse<T>: Sized {
    type Err;
    fn try_from(T) -> Result<Self, Self::Err>;
}

#[derive(Fail, Debug)]
pub enum ParseError {
    #[fail(display = "Parameter {} not found", arg)]
    ParameterNotFound {
        arg: &'static str,
    },
    #[fail(display = "Command {} is unknown", _0)]
    UnknownCommand(String),
    #[fail(display = "Cannot parse \"{}\" as int for parameter {} ({})", value,
        arg, error)]
    ParseInt {
        arg: &'static str,
        value: String,
        error: ParseIntError,
    },
    #[fail(display = "Cannot parse \"{}\" as float for parameter {} ({})",
        value, arg, error)]
    ParseFloat {
        arg: &'static str,
        value: String,
        error: ParseFloatError,
    },
    #[fail(display = "Cannot parse \"{}\" as bool for parameter {}", value,
        arg)]
    ParseBool {
        arg: &'static str,
        value: String,
    },
    #[fail(display = "Invalid value \"{}\" for parameter {}", value, arg)]
    InvalidValue {
        arg: &'static str,
        value: String,
    },
}

include!(concat!(env!("OUT_DIR"), "/messages.rs"));
