extern crate chrono;
#[macro_use]
extern crate failure;
extern crate futures;
extern crate num;
#[macro_use]
extern crate num_derive;
extern crate tsproto;

pub mod errors;
pub mod messages;
pub mod permissions;
pub mod versions;

use std::fmt;

/// A `ClientId` identifies a client which is connected to a server.
///
/// Every client that we see on a server has a `ClientId`, even our own
/// connection.
#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub struct ClientId(pub u16);
/// Describes a client or server uid which is a base64
/// encoded hash or a special reserved name.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Uid(pub String);

/// The database id of a client.
///
/// This is the id which is saved for a client in the database of one specific
/// server.
#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub struct ClientDbId(pub u64);

/// Identifies a channel on a server.
#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub struct ChannelId(pub u64);

/// Identifies a server group on a server.
#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub struct ServerGroupId(pub u64);

/// Identifies a channel group on a server.
#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub struct ChannelGroupId(pub u64);

#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub struct IconHash(pub u32);

#[derive(
	Debug, PartialEq, Eq, Clone, Copy, Hash, FromPrimitive, ToPrimitive,
)]
pub enum TextMessageTargetMode {
	/// Maybe to all servers?
	Unknown,
	/// Send to specific client
	Client,
	/// Send to current channel
	Channel,
	/// Send to server chat
	Server,
}

#[derive(
	Debug, PartialEq, Eq, Clone, Copy, Hash, FromPrimitive, ToPrimitive,
)]
pub enum HostMessageMode {
	/// Dont display anything
	None,
	/// Display message inside log
	Log,
	/// Display message inside a modal dialog
	Modal,
	/// Display message inside a modal dialog and quit/close server/connection
	Modalquit,
}

#[derive(
	Debug, PartialEq, Eq, Clone, Copy, Hash, FromPrimitive, ToPrimitive,
)]
pub enum HostBannerMode {
	/// Do not adjust
	NoAdjust,
	/// Adjust and ignore aspect ratio
	AdjustIgnoreAspect,
	/// Adjust and keep aspect ratio
	AdjustKeepAspect,
}

#[derive(
	Debug, PartialEq, Eq, Clone, Copy, Hash, FromPrimitive, ToPrimitive,
)]
pub enum Codec {
	/// Mono,   16bit,  8kHz, bitrate dependent on the quality setting
	SpeexNarrowband,
	/// Mono,   16bit, 16kHz, bitrate dependent on the quality setting
	SpeexWideband,
	/// Mono,   16bit, 32kHz, bitrate dependent on the quality setting
	SpeexUltrawideband,
	/// Mono,   16bit, 48kHz, bitrate dependent on the quality setting
	CeltMono,
	/// Mono,   16bit, 48kHz, bitrate dependent on the quality setting, optimized for voice
	OpusVoice,
	/// Stereo, 16bit, 48kHz, bitrate dependent on the quality setting, optimized for music
	OpusMusic,
}

#[derive(
	Debug, PartialEq, Eq, Clone, Copy, Hash, FromPrimitive, ToPrimitive,
)]
pub enum CodecEncryptionMode {
	/// Voice encryption is configured per channel
	PerChannel,
	/// Voice encryption is globally off
	ForcedOff,
	/// Voice encryption is globally on
	ForcedOn,
}

#[derive(
	Debug, PartialEq, Eq, Clone, Copy, Hash, FromPrimitive, ToPrimitive,
)]
pub enum Reason {
	/// No reason data
	None,
	/// Has invoker
	Moved,
	/// No reason data
	Subscription,
	LostConnection,
	/// Has invoker
	KickChannel,
	/// Has invoker
	KickServer,
	/// Has invoker, bantime
	KickServerBan,
	Serverstop,
	Clientdisconnect,
	/// No reason data
	Channelupdate,
	/// Has invoker
	Channeledit,
	ClientdisconnectServerShutdown,
}

#[derive(
	Debug, PartialEq, Eq, Clone, Copy, Hash, FromPrimitive, ToPrimitive,
)]
pub enum ClientType {
	Normal,
	/// Server query client
	Query,
}

#[derive(
	Debug, PartialEq, Eq, Clone, Copy, Hash, FromPrimitive, ToPrimitive,
)]
pub enum GroupNamingMode {
	/// No group name is displayed.
	None,
	/// Group name is displayed before the client name.
	Before,
	/// Group name is displayed after the client name.
	After,
}

#[derive(
	Debug, PartialEq, Eq, Clone, Copy, Hash, FromPrimitive, ToPrimitive,
)]
pub enum GroupType {
	/// Template group (used for new virtual servers).
	Template,
	/// Regular group (used for regular clients).
	Regular,
	/// Global query group (used for server query clients).
	Query,
}

#[derive(
	Debug, PartialEq, Eq, Clone, Copy, Hash, FromPrimitive, ToPrimitive,
)]
pub enum LicenseType {
	/// No licence
	NoLicense = 0,
	/// Authorised TeamSpeak Host Provider License (ATHP)
	Athp = 1,
	/// Offline/LAN License
	Lan = 2,
	/// Non-Profit License (NPL)
	Npl = 3,
	/// Unknown License
	Unknown = 4,
}

impl fmt::Display for ClientId {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "{}", self.0)
	}
}
impl fmt::Display for Uid {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "{}", self.0)
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
