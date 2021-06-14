//! `tsproto-packets` parses and serializes TeamSpeak packets and commands.

use std::fmt;

use thiserror::Error;

pub mod commands;
pub mod packets;

type Result<T, E = Error> = std::result::Result<T, E>;

pub const S2C_HEADER_LEN: usize = 11;
pub const C2S_HEADER_LEN: usize = 13;

#[derive(Error, Debug)]
#[non_exhaustive]
pub enum Error {
	#[error(transparent)]
	Base64(#[from] base64::DecodeError),
	#[error(transparent)]
	Io(#[from] std::io::Error),
	#[error(transparent)]
	ParseInt(#[from] std::num::ParseIntError),
	#[error(transparent)]
	Utf8(#[from] std::str::Utf8Error),
	#[error(transparent)]
	StringUtf8(#[from] std::string::FromUtf8Error),

	#[error("Invalid init step {0}")]
	InvalidInitStep(u8),
	#[error("Invalid audio codec {0}")]
	InvalidCodec(u8),
	#[error("Packet content is too short (length {0})")]
	PacketContentTooShort(usize),
	#[error("Packet is too short (length {0})")]
	PacketTooShort(usize),
	#[error("Cannot parse command ({0})")]
	ParseCommand(String),
	#[error("Got a packet with unknown type ({0})")]
	UnknownPacketType(u8),
	#[error("Tried to parse a packet from the wrong direction")]
	WrongDirection,
	#[error("Wrong mac, expected TS3INIT1 but got {0:?}")]
	WrongInitMac(Vec<u8>),
	#[error("Wrong packet type ({0:?})")]
	WrongPacketType(packets::PacketType),
}

pub struct HexSlice<'a, T: fmt::LowerHex + 'a>(pub &'a [T]);

impl<'a> fmt::Display for HexSlice<'a, u8> {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "Hex[")?;
		if let Some((l, m)) = self.0.split_last() {
			for b in m {
				write!(f, "{:02x} ", b)?;
			}
			write!(f, "{:02x}", l)?;
		}
		write!(f, "]")
	}
}
