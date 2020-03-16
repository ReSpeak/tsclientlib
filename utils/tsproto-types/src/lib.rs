use std::fmt;
use std::u64;

use bitflags::bitflags;
use num_derive::{FromPrimitive, ToPrimitive};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

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
pub struct Uid(pub Vec<u8>);

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct UidRef<'a>(pub &'a [u8]);
impl<'a> Into<Uid> for UidRef<'a> {
	fn into(self) -> Uid { Uid(self.0.into()) }
}
impl Uid {
	pub fn as_ref(&self) -> UidRef { UidRef(self.0.as_ref()) }

	/// TeamSpeak uses a different encoding of the uid for fetching avatars.
	///
	/// The raw data (base64-decoded) is encoded in hex, but instead of using
	/// [0-9a-f] with [a-p].
	pub fn as_avatar(&self) -> String { self.as_ref().as_avatar() }

	pub fn is_server_admin(&self) -> bool { self.as_ref().is_server_admin() }
}

impl UidRef<'_> {
	/// TeamSpeak uses a different encoding of the uid for fetching avatars.
	///
	/// The raw data (base64-decoded) is encoded in hex, but instead of using
	/// [0-9a-f] with [a-p].
	pub fn as_avatar(&self) -> String {
		let mut res = String::with_capacity(self.0.len() * 2);
		for b in self.0 {
			res.push((b'a' + (b >> 4)) as char);
			res.push((b'a' + (b & 0xf)) as char);
		}
		res
	}

	pub fn is_server_admin(&self) -> bool { self.0 == b"ServerAdmin" }
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
pub struct IconHash(pub u32);

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct Permission(pub u32);
impl Permission {
	/// Never fails
	pub fn from_u32(i: u32) -> Option<Self> { Some(Permission(i)) }
	/// Never fails
	pub fn to_u32(&self) -> Option<u32> { Some(self.0) }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub enum ClientType {
	Normal,
	/// Server query client
	Query { admin: bool }
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
	pub uid: Option<Uid>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct InvokerRef<'a> {
	pub name: &'a str,
	pub id: ClientId,
	pub uid: Option<UidRef<'a>>,
}

impl Invoker {
	pub fn as_ref(&self) -> InvokerRef {
		InvokerRef {
			name: &self.name,
			id: self.id,
			uid: self.uid.as_ref().map(|u| u.as_ref()),
		}
	}
}

impl fmt::Display for ClientId {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "{}", self.0)
	}
}
impl fmt::Display for Uid {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "{}", base64::encode(&self.0))
	}
}
impl fmt::Display for ClientDbId {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "{}", self.0)
	}
}
impl fmt::Display for ChannelId {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "{}", self.0)
	}
}
impl fmt::Display for ServerGroupId {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "{}", self.0)
	}
}
impl fmt::Display for ChannelGroupId {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "{}", self.0)
	}
}
