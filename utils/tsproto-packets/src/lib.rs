#[macro_use]
extern crate rental;

use std::fmt;

use derive_more::From;
use failure::{Fail, ResultExt};

pub mod commands;
pub mod packets;

type Result<T> = std::result::Result<T, Error>;

pub const S2C_HEADER_LEN: usize = 11;
pub const C2S_HEADER_LEN: usize = 13;

#[derive(Fail, Debug, From)]
#[non_exhaustive]
pub enum Error {
	#[fail(display = "{}", _0)]
	Base64(#[cause] base64::DecodeError),
	#[fail(display = "{}", _0)]
	Io(#[cause] std::io::Error),
	#[fail(display = "{}", _0)]
	ParseInt(#[cause] std::num::ParseIntError),
	#[fail(display = "{}", _0)]
	Utf8(#[cause] std::str::Utf8Error),

	#[fail(
		display = "Packet {} not in receive window [{};{}) for type {:?}",
		id, next, limit, p_type
	)]
	NotInReceiveWindow {
		id: u16,
		next: u16,
		limit: u16,
		p_type: packets::PacketType,
	},
	#[fail(display = "{}", _0)]
	#[from(ignore)]
	ParsePacket(String),
	#[fail(display = "Got unallowed unencrypted packet")]
	UnallowedUnencryptedPacket,
	#[fail(display = "Got unexpected init packet")]
	UnexpectedInitPacket,
	/// Store packet type, generation id and packet id.
	#[fail(display = "{:?} Packet {}:{} has a wrong mac", _0, _1, _2)]
	WrongMac(packets::PacketType, u32, u16),
	#[fail(display = "Got a packet with unknown type ({})", _0)]
	UnknownPacketType(u8),
	#[fail(display = "Maximum length exceeded for {}", _0)]
	#[from(ignore)]
	MaxLengthExceeded(String),
	#[fail(display = "Cannot parse command ({})", _0)]
	#[from(ignore)]
	ParseCommand(String),
	#[fail(display = "Wrong signature")]
	WrongSignature,
	#[fail(display = "{}", _0)]
	Other(#[cause] failure::Compat<failure::Error>),
}

impl From<failure::Error> for Error {
	fn from(e: failure::Error) -> Self {
		let r: std::result::Result<(), _> = Err(e);
		Error::Other(r.compat().unwrap_err())
	}
}

pub struct HexSlice<'a, T: fmt::LowerHex + 'a>(pub &'a [T]);

impl<'a> fmt::Debug for HexSlice<'a, u8> {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "[")?;
		if let Some((l, m)) = self.0.split_last() {
			for b in m {
				if *b == 0 {
					write!(f, "0, ")?;
				} else {
					write!(f, "{:#02x}, ", b)?;
				}
			}
			if *l == 0 {
				write!(f, "0")?;
			} else {
				write!(f, "{:#02x}", l)?;
			}
		}
		write!(f, "]")
	}
}
