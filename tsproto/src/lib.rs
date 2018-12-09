// TODO remove?
#![cfg_attr(
	feature = "cargo-clippy",
	allow(
		redundant_closure_call,
		clone_on_ref_ptr,
		let_and_return,
		useless_format
	)
)]

extern crate simple_asn1;
#[macro_use]
extern crate arrayref;
extern crate base64;
#[macro_use]
extern crate bitflags;
extern crate byteorder;
extern crate bytes;
extern crate chrono;
extern crate curve25519_dalek;
#[macro_use]
extern crate derive_more;
#[macro_use]
extern crate failure;
extern crate futures;
#[cfg(feature = "rug")]
extern crate rug;
#[macro_use]
extern crate nom;
extern crate num_bigint;
#[macro_use]
extern crate num_derive;
extern crate num_traits;
extern crate openssl;
extern crate parking_lot;
extern crate quicklz;
extern crate rand;
#[macro_use]
extern crate rental;
extern crate ring;
#[macro_use]
extern crate slog;
extern crate slog_async;
extern crate slog_perf;
extern crate slog_term;
extern crate stable_deref_trait;
extern crate tokio;
extern crate tokio_threadpool;

use std::net::SocketAddr;

use failure::ResultExt;

pub mod algorithms;
pub mod client;
pub mod commands;
pub mod connection;
pub mod connectionmanager;
pub mod crypto;
pub mod handler_data;
pub mod license;
pub mod log;
pub mod packet_codec;
pub mod packets;
pub mod resend;
pub mod utils;

type Result<T> = std::result::Result<T, Error>;
type LockedHashMap<K, V> = std::sync::Arc<parking_lot::RwLock<std::collections::HashMap<K, V>>>;

/// The maximum number of bytes for a fragmented packet.
#[cfg_attr(feature = "cargo-clippy", allow(unreadable_literal))]
const MAX_FRAGMENTS_LENGTH: usize = 40960;
/// The maximum number of packets which are stored, if they are received
/// out-of-order.
const MAX_QUEUE_LEN: usize = 50;
/// The maximum decompressed size of a packet.
#[cfg_attr(feature = "cargo-clippy", allow(unreadable_literal))]
const MAX_DECOMPRESSED_SIZE: u32 = 40960;
const FAKE_KEY: [u8; 16] = *b"c:\\windows\\syste";
const FAKE_NONCE: [u8; 16] = *b"m\\firewall32.cpl";
/// The root key in the TeamSpeak license system.
const ROOT_KEY: [u8; 32] = [
	0xcd, 0x0d, 0xe2, 0xae, 0xd4, 0x63, 0x45, 0x50, 0x9a, 0x7e, 0x3c, 0xfd,
	0x8f, 0x68, 0xb3, 0xdc, 0x75, 0x55, 0xb2, 0x9d, 0xcc, 0xec, 0x73, 0xcd,
	0x18, 0x75, 0x0f, 0x99, 0x38, 0x12, 0x40, 0x8a,
];
/// Xored onto saved identities in the TeamSpeak client settings file.
const IDENTITY_OBFUSCATION: [u8; 128] = *b"b9dfaa7bee6ac57ac7b65f1094a1c155\
	e747327bc2fe5d51c512023fe54a280201004e90ad1daaae1075d53b7d571c30e063b5a\
	62a4a017bb394833aa0983e6e";
const UDP_SINK_CAPACITY: usize = 20;
const S2C_HEADER_LEN: usize = 11;
const C2S_HEADER_LEN: usize = 13;

#[derive(Fail, Debug, From)]
pub enum Error {
	#[fail(display = "{}", _0)]
	Asn1Decode(#[cause] simple_asn1::ASN1DecodeErr),
	#[fail(display = "{}", _0)]
	Asn1Encode(#[cause] simple_asn1::ASN1EncodeErr),
	#[fail(display = "{}", _0)]
	Base64(#[cause] base64::DecodeError),
	#[fail(display = "{}", _0)]
	FutureCanceled(#[cause] futures::Canceled),
	#[fail(display = "{}", _0)]
	Io(#[cause] std::io::Error),
	#[fail(display = "{}", _0)]
	ParseInt(#[cause] std::num::ParseIntError),
	#[fail(display = "{}", _0)]
	Openssl(#[cause] openssl::error::ErrorStack),
	#[fail(display = "{}", _0)]
	Quicklz(#[cause] quicklz::Error),
	#[fail(display = "{}", _0)]
	Rand(#[cause] rand::Error),
	#[fail(display = "{}", _0)]
	Ring(#[cause] ring::error::Unspecified),
	#[fail(display = "{}", _0)]
	Timer(#[cause] tokio::timer::Error),
	#[fail(display = "{}", _0)]
	Utf8(#[cause] std::str::Utf8Error),

	#[fail(
		display = "Packet {} not in receive window [{};{}) for type {:?}",
		id,
		next,
		limit,
		p_type
	)]
	NotInReceiveWindow {
		id: u16,
		next: u16,
		limit: u16,
		p_type: packets::PacketType,
	},
	#[fail(display = "{}", _0)]
	ParsePacket(String),
	#[fail(display = "Got unallowed unencrypted packet")]
	UnallowedUnencryptedPacket,
	#[fail(display = "Got unexpected init packet")]
	UnexpectedInitPacket,
	#[fail(display = "Wrong mac")]
	WrongMac,
	#[fail(display = "Got a packet with unknown type ({})", _0)]
	UnknownPacketType(u8),
	#[fail(display = "Maximum length exceeded for {}", _0)]
	MaxLengthExceeded(String),
	#[fail(display = "Cannot parse command ({})", _0)]
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

#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub struct ClientId(pub SocketAddr);
#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub struct ServerId(pub SocketAddr);

impl Into<SocketAddr> for ClientId {
	fn into(self) -> SocketAddr {
		self.0
	}
}
impl Into<SocketAddr> for ServerId {
	fn into(self) -> SocketAddr {
		self.0
	}
}

pub fn init() -> Result<()> {
	openssl::init();
	Ok(())
}
