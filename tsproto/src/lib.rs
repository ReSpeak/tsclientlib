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

use std::fmt;
use std::num::ParseIntError;

use base64::prelude::*;
use serde::de::{Unexpected, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;
use tsproto_packets::packets;
use tsproto_types::crypto::EccKeyPrivP256;

pub mod algorithms;
pub mod client;
pub mod connection;
pub mod license;
pub mod log;
pub mod packet_codec;
pub mod resend;
pub mod utils;

use algorithms as algs;

// The build environment of tsproto.
git_testament::git_testament!(TESTAMENT);
#[doc(hidden)]
pub fn get_testament() -> &'static git_testament::GitTestament<'static> { &TESTAMENT }

type Result<T> = std::result::Result<T, Error>;

/// The maximum number of bytes for a fragmented packet.
///
/// The maximum packet size is 500 bytes, as used by
/// `algorithms::compress_and_split`.
/// We pick the ethernet MTU for possible future compatibility, it is unlikely
/// that a packet will get bigger.
pub const MAX_UDP_PACKET_LENGTH: usize = 1500;

/// The maximum number of bytes for a fragmented packet.
#[allow(clippy::unreadable_literal)]
const MAX_FRAGMENTS_LENGTH: usize = 40960;

/// The maximum number of packets which are stored, if they are received
/// out-of-order.
const MAX_QUEUE_LEN: u16 = 200;

/// The maximum decompressed size of a packet.
#[allow(clippy::unreadable_literal)]

/// On large servers with more than 2000 channels, the notifychannelsubscribed packet can be
/// 50 kB large (uncompressed). A maximum size to 2 MiB should allow for even larger servers.
const MAX_DECOMPRESSED_SIZE: u32 = 2 * 1024 * 1024;
const FAKE_KEY: [u8; 16] = *b"c:\\windows\\syste";
const FAKE_NONCE: [u8; 16] = *b"m\\firewall32.cpl";

/// The root key in the TeamSpeak license system.
pub const ROOT_KEY: [u8; 32] = [
	0xcd, 0x0d, 0xe2, 0xae, 0xd4, 0x63, 0x45, 0x50, 0x9a, 0x7e, 0x3c, 0xfd, 0x8f, 0x68, 0xb3, 0xdc,
	0x75, 0x55, 0xb2, 0x9d, 0xcc, 0xec, 0x73, 0xcd, 0x18, 0x75, 0x0f, 0x99, 0x38, 0x12, 0x40, 0x8a,
];

/// The maximum amount of ack pachets that a connection intermediately stores.
///
/// When this amount is stored, no new packets will be polled from the UDP
/// connection.
const UDP_SINK_CAPACITY: usize = 50;

#[derive(Error, Debug)]
#[non_exhaustive]
pub enum Error {
	#[error("Failed to create ack packet: {0}")]
	CreateAck(#[source] tsproto_packets::Error),
	#[error("Failed to decompress packet: {0}")]
	DecompressPacket(#[source] quicklz::Error),
	#[error(transparent)]
	IdentityCrypto(tsproto_types::crypto::Error),
	#[error("Failed to parse int: {0}")]
	InvalidHex(#[source] ParseIntError),
	#[error("Maximum length exceeded for {0}")]
	MaxLengthExceeded(&'static str),
	#[error("Network error: {0}")]
	Network(#[source] std::io::Error),
	#[error("Packet {id} not in receive window [{next};{limit}) for type {p_type:?}")]
	NotInReceiveWindow { id: u16, next: u16, limit: u16, p_type: packets::PacketType },
	#[error("Failed to parse {0} packet: {1}")]
	PacketParse(&'static str, #[source] tsproto_packets::Error),
	#[error("Connection timed out: {0}")]
	Timeout(&'static str),
	#[error("Got unallowed unencrypted packet")]
	UnallowedUnencryptedPacket,
	#[error("Got unexpected init packet")]
	UnexpectedInitPacket,
	#[error("Packet has wrong client id {0}")]
	WrongClientId(u16),
	#[error("Received udp packet from wrong address")]
	WrongAddress,
	#[error("{p_type:?} Packet {generation_id}:{packet_id} has a wrong mac")]
	WrongMac { p_type: packets::PacketType, generation_id: u32, packet_id: u16 },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Identity {
	#[serde(serialize_with = "serialize_id_key", deserialize_with = "deserialize_id_key")]
	key: EccKeyPrivP256,
	/// The `client_key_offset`/counter for hash cash.
	counter: u64,
	/// The maximum counter that was tried, this is greater or equal to
	/// `counter` but may yield a lower level.
	max_counter: u64,
}

struct IdKeyVisitor;
impl<'de> Visitor<'de> for IdKeyVisitor {
	type Value = EccKeyPrivP256;

	fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "a P256 private ecc key")
	}

	fn visit_str<E: serde::de::Error>(self, s: &str) -> std::result::Result<Self::Value, E> {
		EccKeyPrivP256::import_str(s)
			.map_err(|_| serde::de::Error::invalid_value(Unexpected::Str(s), &self))
	}
}

fn serialize_id_key<S: Serializer>(
	key: &EccKeyPrivP256, s: S,
) -> std::result::Result<S::Ok, S::Error> {
	s.serialize_str(&BASE64_STANDARD.encode(key.to_short()))
}

fn deserialize_id_key<'de, D: Deserializer<'de>>(
	d: D,
) -> std::result::Result<EccKeyPrivP256, D::Error> {
	d.deserialize_str(IdKeyVisitor)
}

impl Identity {
	#[inline]
	pub fn create() -> Self {
		let mut res = Self::new(EccKeyPrivP256::create(), 0);
		res.upgrade_level(8);
		res
	}

	#[inline]
	pub fn new(key: EccKeyPrivP256, counter: u64) -> Self {
		Self::new_with_max_counter(key, counter, counter)
	}

	#[inline]
	pub fn new_with_max_counter(key: EccKeyPrivP256, counter: u64, max_counter: u64) -> Self {
		Self { key, counter, max_counter }
	}

	#[inline]
	pub fn new_from_str(key: &str) -> Result<Self> {
		if let Ok(identity) = Identity::new_from_ts_str(key) {
			return Ok(identity);
		}
		let mut res = Self::new(EccKeyPrivP256::import_str(key).map_err(Error::IdentityCrypto)?, 0);
		res.upgrade_level(8);
		Ok(res)
	}

	pub fn new_from_ts_str(key: &str) -> Result<Self> {
		let counter_separator = key
			.find('V')
			.ok_or(Error::IdentityCrypto(tsproto_types::crypto::Error::NoCounterBlock))?;
		let counter = key[..counter_separator]
			.parse::<u64>()
			.map_err(|_| Error::IdentityCrypto(tsproto_types::crypto::Error::NoCounterBlock))?;
		let ecc_key = EccKeyPrivP256::import_str(&key[(counter_separator + 1)..])
			.map_err(Error::IdentityCrypto)?;
		Ok(Self::new(ecc_key, counter))
	}

	#[inline]
	pub fn new_from_bytes(key: &[u8]) -> Result<Self> {
		let mut res = Self::new(EccKeyPrivP256::import(key).map_err(Error::IdentityCrypto)?, 0);
		res.upgrade_level(8);
		Ok(res)
	}

	#[inline]
	pub fn key(&self) -> &EccKeyPrivP256 { &self.key }
	#[inline]
	pub fn counter(&self) -> u64 { self.counter }
	#[inline]
	pub fn max_counter(&self) -> u64 { self.max_counter }

	#[inline]
	pub fn set_key(&mut self, key: EccKeyPrivP256) { self.key = key }
	#[inline]
	pub fn set_counter(&mut self, counter: u64) { self.counter = counter; }
	#[inline]
	pub fn set_max_counter(&mut self, max_counter: u64) { self.max_counter = max_counter; }

	/// Compute the current hash cash level.
	#[inline]
	pub fn level(&self) -> u8 {
		let omega = self.key.to_pub().to_ts();
		algs::get_hash_cash_level(&omega, self.counter)
	}

	/// Compute a better hash cash level.
	pub fn upgrade_level(&mut self, target: u8) {
		let omega = self.key.to_pub().to_ts();
		let mut offset = self.max_counter;
		while offset < u64::MAX && algs::get_hash_cash_level(&omega, offset) < target {
			offset += 1;
		}
		self.counter = offset;
		self.max_counter = offset;
	}
}

#[cfg(test)]
mod identity_tests {
	use super::*;

	const TEST_PRIV_KEY: &str =
		"MG8DAgeAAgEgAiEA6rtKxDn/o/Bo50rNtAE5Ph3h2RKLHQ0gbFkvm2yA79kCIQCrfzAZts/\
		 vHP+3MOetKLjNnpZXt4c6U3UB4gWLKR4H9AIgYTyJofmztcTBjq3KZcDdxu+G4RPVwE5vg8VaN2jbQao=";
	const TEST_UID: &str = "test/9PZ9vww/Bpf5vJxtJhpz80=";

	#[test]
	fn parse_ts_base64() {
		let identity = Identity::new_from_str(TEST_PRIV_KEY).unwrap();
		let uid = identity.key().to_pub().get_uid();
		assert_eq!(TEST_UID, &uid);
	}

	#[test]
	fn parse_ts_base64_with_offset() {
		let ident_str = String::from("2792354V") + TEST_PRIV_KEY;
		let identity = Identity::new_from_str(&ident_str).unwrap();
		let uid = identity.key().to_pub().get_uid();
		assert_eq!(TEST_UID, &uid);
		assert_eq!(identity.level(), 21u8);
	}
}
