//! `tsproto-types` contains basic types and enums that are used within the TeamSpeak protocol.

use std::borrow::{Borrow, Cow};
use std::fmt;

use base64::prelude::*;
use bitflags::bitflags;
use num_derive::{FromPrimitive, ToPrimitive};
use ref_cast::RefCast;
use serde::{Deserialize, Deserializer, Serialize};
use time::OffsetDateTime;

pub mod crypto;
pub mod errors;
pub mod versions;

include!(concat!(env!("OUT_DIR"), "/enums.rs"));

/// A `ClientId` identifies a client which is connected to a server.
///
/// Every client that we see on a server has a `ClientId`, even our own
/// connection.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct ClientId(pub u16);
/// Describes a client or server uid which is a base64
/// encoded hash or a special reserved name.
///
/// This is saved raw, so the base64-decoded TeamSpeak uid.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct UidBuf(pub Vec<u8>);

#[derive(Debug, Eq, PartialEq, RefCast, Serialize)]
#[repr(transparent)]
pub struct Uid(pub [u8]);

impl ToOwned for Uid {
	type Owned = UidBuf;
	fn to_owned(&self) -> Self::Owned { UidBuf(self.0.to_owned()) }
}
impl Borrow<Uid> for UidBuf {
	fn borrow(&self) -> &Uid { Uid::ref_cast(self.0.borrow()) }
}
impl AsRef<Uid> for UidBuf {
	fn as_ref(&self) -> &Uid { self.borrow() }
}
impl core::ops::Deref for UidBuf {
	type Target = Uid;
	fn deref(&self) -> &Self::Target { self.borrow() }
}
impl<'a> From<&'a Uid> for Cow<'a, Uid> {
	fn from(u: &'a Uid) -> Self { Cow::Borrowed(u) }
}

impl Uid {
	pub fn from_bytes(data: &'_ [u8]) -> &Self { Uid::ref_cast(data) }

	/// TeamSpeak uses a different encoding of the uid for fetching avatars.
	///
	/// The raw data (base64-decoded) is encoded in hex, but instead of using
	/// [0-9a-f] with [a-p].
	pub fn as_avatar(&self) -> String {
		let mut res = String::with_capacity(self.0.len() * 2);
		for b in &self.0 {
			res.push((b'a' + (b >> 4)) as char);
			res.push((b'a' + (b & 0xf)) as char);
		}
		res
	}

	pub fn is_server_admin(&self) -> bool { &self.0 == b"ServerAdmin" }
}

impl<'a, 'de: 'a> Deserialize<'de> for &'a Uid {
	fn deserialize<D>(d: D) -> Result<&'a Uid, D::Error>
	where D: Deserializer<'de> {
		let data = <&[u8]>::deserialize(d)?;
		Ok(Uid::from_bytes(data))
	}
}

/// The database id of a client.
///
/// This is the id which is saved for a client in the database of one specific
/// server.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct ClientDbId(pub u64);

/// Identifies a channel on a server.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct ChannelId(pub u64);

/// Identifies a server group on a server.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct ServerGroupId(pub u64);

/// Identifies a channel group on a server.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct ChannelGroupId(pub u64);

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct IconId(pub u32);

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct Permission(pub u32);
impl Permission {
	/// Never fails
	pub fn from_u32(i: u32) -> Option<Self> { Some(Permission(i)) }
	/// Never fails
	pub fn to_u32(self) -> Option<u32> { Some(self.0) }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum ClientType {
	Normal,
	/// Server query client
	Query {
		admin: bool,
	},
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum MaxClients {
	Unlimited,
	Inherited,
	Limited(u16),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TalkPowerRequest {
	pub time: OffsetDateTime,
	pub message: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Invoker {
	pub name: String,
	pub id: ClientId,
	pub uid: Option<UidBuf>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct InvokerRef<'a> {
	pub name: &'a str,
	pub id: ClientId,
	pub uid: Option<&'a Uid>,
}

impl Invoker {
	pub fn as_ref(&self) -> InvokerRef {
		InvokerRef { name: &self.name, id: self.id, uid: self.uid.as_ref().map(|u| u.as_ref()) }
	}
}

impl fmt::Display for ClientId {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result { write!(f, "{}", self.0) }
}
impl fmt::Display for Uid {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "{}", BASE64_STANDARD.encode(&self.0))
	}
}
impl fmt::Display for ClientDbId {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result { write!(f, "{}", self.0) }
}
impl fmt::Display for ChannelId {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result { write!(f, "{}", self.0) }
}
impl fmt::Display for ServerGroupId {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result { write!(f, "{}", self.0) }
}
impl fmt::Display for ChannelGroupId {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result { write!(f, "{}", self.0) }
}
