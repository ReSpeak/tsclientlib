//! `ts-bookkeeping` contains structs to store the state of a TeamSpeak server, with its clients and
//! channels.
//!
//! The crate can be used to keep track of the state on a server by processing all incoming
//! commands, which is why it is called “bookkeeping”. It also contains generated structs for all
//! TeamSpeak commands.
//!
//! Incoming commands can be applied to the state and generate [`events`]. The main struct is
//! [`data::Connection`].
//!
//! The structs have methods to create packets for various actions. The generated packets can be
//! sent to a server.

use std::fmt;
use std::net::{IpAddr, SocketAddr};

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub mod data;
pub mod events;
pub mod messages;

// Reexports
pub use tsproto_types::errors::Error as TsError;
pub use tsproto_types::versions::Version;
pub use tsproto_types::{
	ChannelGroupId, ChannelId, ChannelPermissionHint, ChannelType, ClientDbId, ClientId,
	ClientPermissionHint, ClientType, Codec, CodecEncryptionMode, GroupNamingMode, GroupType,
	HostBannerMode, HostMessageMode, IconId, Invoker, InvokerRef, LicenseType, LogLevel,
	MaxClients, Permission, PermissionType, PluginTargetMode, Reason, ServerGroupId,
	TalkPowerRequest, TextMessageTargetMode, TokenType, Uid, UidBuf,
};

type Result<T> = std::result::Result<T, Error>;

#[derive(Error, Debug)]
#[non_exhaustive]
pub enum Error {
	#[error("Target client id missing for a client text message")]
	MessageWithoutTargetClientId,
	#[error("Unknown TextMessageTargetMode")]
	UnknownTextMessageTargetMode,
	#[error("{0} {1} not found")]
	NotFound(&'static str, String),
	#[error("{0} should be removed but does not exist")]
	RemoveNotFound(&'static str),
	#[error("Failed to parse connection ip: {0}")]
	InvalidConnectionIp(#[source] std::net::AddrParseError),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum ServerAddress {
	SocketAddr(SocketAddr),
	Other(String),
}

impl From<SocketAddr> for ServerAddress {
	fn from(addr: SocketAddr) -> Self { ServerAddress::SocketAddr(addr) }
}

impl From<String> for ServerAddress {
	fn from(addr: String) -> Self { ServerAddress::Other(addr) }
}

impl<'a> From<&'a str> for ServerAddress {
	fn from(addr: &'a str) -> Self { ServerAddress::Other(addr.to_string()) }
}

impl fmt::Display for ServerAddress {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		match self {
			ServerAddress::SocketAddr(a) => fmt::Display::fmt(a, f),
			ServerAddress::Other(a) => fmt::Display::fmt(a, f),
		}
	}
}

/// All possible targets to send messages.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum MessageTarget {
	Server,
	Channel,
	Client(ClientId),
	Poke(ClientId),
}

/// The configuration to create a new connection.
#[derive(Deserialize, Serialize)]
pub struct ConnectOptions {
	address: ServerAddress,
	local_address: Option<SocketAddr>,
	name: String,
	version: Version,
	log_commands: bool,
	log_packets: bool,
	log_udp_packets: bool,
}

impl ConnectOptions {
	/// Start creating the configuration of a new connection.
	///
	/// # Arguments
	/// The address of the server has to be supplied. The address can be a
	/// [`SocketAddr`](std::net::SocketAddr), a string or directly a [`ServerAddress`]. A string
	/// will automatically be resolved from all formats supported by TeamSpeak.
	/// For details, see [`resolver::resolve`].
	#[inline]
	pub fn new<A: Into<ServerAddress>>(address: A) -> Self {
		Self {
			address: address.into(),
			local_address: None,
			name: String::from("TeamSpeakUser"),
			version: Version::Linux_3_2_1,
			log_commands: false,
			log_packets: false,
			log_udp_packets: false,
		}
	}

	/// The address for the socket of our client
	///
	/// # Default
	/// The default is `0.0.0:0` when connecting to an IPv4 address and `[::]:0`
	/// when connecting to an IPv6 address.
	#[inline]
	pub fn local_address(mut self, local_address: SocketAddr) -> Self {
		self.local_address = Some(local_address);
		self
	}

	/// The name of the user.
	///
	/// # Default
	/// `TeamSpeakUser`
	#[inline]
	pub fn name(mut self, name: String) -> Self {
		self.name = name;
		self
	}

	/// The displayed version of the client.
	///
	/// # Default
	/// `3.2.1 on Linux`
	#[inline]
	pub fn version(mut self, version: Version) -> Self {
		self.version = version;
		self
	}

	/// If the content of all commands should be written to the logger.
	///
	/// # Default
	/// `false`
	#[inline]
	pub fn log_commands(mut self, log_commands: bool) -> Self {
		self.log_commands = log_commands;
		self
	}

	/// If the content of all packets in high-level form should be written to
	/// the logger.
	///
	/// # Default
	/// `false`
	#[inline]
	pub fn log_packets(mut self, log_packets: bool) -> Self {
		self.log_packets = log_packets;
		self
	}

	/// If the content of all udp packets in byte-array form should be written
	/// to the logger.
	///
	/// # Default
	/// `false`
	#[inline]
	pub fn log_udp_packets(mut self, log_udp_packets: bool) -> Self {
		self.log_udp_packets = log_udp_packets;
		self
	}
}

impl fmt::Debug for ConnectOptions {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		// Error if attributes are added
		let ConnectOptions {
			address,
			local_address,
			name,
			version,
			log_commands,
			log_packets,
			log_udp_packets,
		} = self;
		write!(
			f,
			"ConnectOptions {{ address: {:?}, local_address: {:?}, name: {}, version: {}, \
			 log_commands: {}, log_packets: {}, log_udp_packets: {} }}",
			address, local_address, name, version, log_commands, log_packets, log_udp_packets,
		)?;
		Ok(())
	}
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DisconnectOptions {
	reason: Option<Reason>,
	message: Option<String>,
}

impl Default for DisconnectOptions {
	#[inline]
	fn default() -> Self { Self { reason: None, message: None } }
}

impl DisconnectOptions {
	#[inline]
	pub fn new() -> Self { Self::default() }

	/// Set the reason for leaving.
	///
	/// # Default
	///
	/// None
	#[inline]
	pub fn reason(mut self, reason: Reason) -> Self {
		self.reason = Some(reason);
		self
	}

	/// Set the leave message.
	///
	/// You also have to set the reason, otherwise the message will not be
	/// displayed.
	///
	/// # Default
	///
	/// None
	#[inline]
	pub fn message<S: Into<String>>(mut self, message: S) -> Self {
		self.message = Some(message.into());
		self
	}
}
