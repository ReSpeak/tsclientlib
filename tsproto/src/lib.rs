//! This library implements the TeamSpeak3 protocol.
//!
//! For a usable library to build clients and bots, you should take a look at
//! [`tsclientlib`](https://github.com/ReSpeak/tsclientlib), which provides a
//! convenient interface to this library.
//!
//! If you are searching for a usable client, [Qint](https://github.com/ReSpeak/Qint)
//! is a cross-platform TeamSpeak client, which is using this library (more
//! correctly, it is using `tsclientlib`).
//!
//! For more info on this project, take a look at the
//! [tsclientlib README](https://github.com/ReSpeak/tsclientlib).

use std::sync::RwLock;

use derive_more::From;
use failure::{Fail, ResultExt};
use tsproto_packets::packets;

pub mod algorithms;
pub mod client;
pub mod connection;
pub mod connectionmanager;
pub mod crypto;
pub mod handler_data;
pub mod license;
pub mod log;
pub mod packet_codec;
pub mod resend;
pub mod utils;

// The build environment of tsproto.
git_testament::git_testament!(TESTAMENT);
#[doc(hidden)]
pub fn get_testament() -> &'static git_testament::GitTestament<'static> {
	&TESTAMENT
}

type Result<T> = std::result::Result<T, Error>;
type LockedHashMap<K, V> =
	std::sync::Arc<RwLock<std::collections::HashMap<K, V>>>;

/// The maximum number of bytes for a fragmented packet.
#[allow(clippy::unreadable_literal)]
const MAX_FRAGMENTS_LENGTH: usize = 40960;
/// The maximum number of packets which are stored, if they are received
/// out-of-order.
const MAX_QUEUE_LEN: u16 = 50;
/// The maximum decompressed size of a packet.
#[allow(clippy::unreadable_literal)]
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

#[derive(Fail, Debug, From)]
#[non_exhaustive]
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
	Quicklz(#[cause] quicklz::Error),
	#[fail(display = "{}", _0)]
	Rand(#[cause] rand::Error),
	#[fail(display = "{}", _0)]
	Timer(#[cause] tokio::timer::Error),
	#[fail(display = "{}", _0)]
	TsprotoPackets(#[cause] tsproto_packets::Error),
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
	#[fail(display = "Got unallowed unencrypted packet")]
	UnallowedUnencryptedPacket,
	#[fail(display = "Got unexpected init packet")]
	UnexpectedInitPacket,
	/// Store packet type, generation id and packet id.
	#[fail(display = "{:?} Packet {}:{} has a wrong mac", _0, _1, _2)]
	WrongMac(packets::PacketType, u32, u16),
	#[fail(display = "Maximum length exceeded for {}", _0)]
	MaxLengthExceeded(String),
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
