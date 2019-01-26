#[macro_use]
extern crate failure;
#[macro_use]
extern crate num_derive;

pub mod errors;
pub mod messages;
pub mod versions;

use std::fmt;

use chrono::{DateTime, Utc};

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

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct UidRef<'a>(pub &'a str);
impl<'a> Into<Uid> for UidRef<'a> {
	fn into(self) -> Uid {
		Uid(self.0.into())
	}
}
impl Uid {
	pub fn as_ref(&self) -> UidRef {
		UidRef(self.0.as_ref())
	}
}

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

#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub struct Permission(pub u32);
impl Permission {
	/// Never fails
	pub fn from_u32(i: u32) -> Option<Self> { Some(Permission(i)) }
	/// Never fails
	pub fn to_u32(&self) -> Option<u32> { Some(self.0) }
}

#[derive(
	Debug, PartialEq, Eq, Clone, Copy, Hash, FromPrimitive, ToPrimitive,
)]
pub enum PermissionType {
	/// Server group permission. (id1: ServerGroupId, id2: 0)
	ServerGroup,
	/// Client specific permission. (id1: ClientDbId, id2: 0)
	GlobalClient,
	/// Channel specific permission. (id1: ChannelId, id2: 0)
	Channel,
	/// Channel group permission. (id1: ChannelId, id2: ChannelGroupId)
	ChannelGroup,
	/// Channel-client specific permission. (id1: ChannelId, id2: ClientDbId)
	ChannelClient,
}

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

#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash, FromPrimitive, ToPrimitive)]
pub enum LicenseType {
	/// No licence
	NoLicense,
	/// Authorised TeamSpeak Host Provider License (ATHP)
	Athp,
	/// Offline/LAN License
	Lan,
	/// Non-Profit License (NPL)
	Npl,
	/// Unknown License
	Unknown,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash, FromPrimitive, ToPrimitive)]
pub enum ChannelType {
	Permanent,
	SemiPermanent,
	Temporary,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash, FromPrimitive, ToPrimitive)]
pub enum TokenType {
	/// Server group token (`id1={groupId}, id2=0`)
	ServerGroup,
	/// Channel group token (`id1={groupId}, id2={channelId}`)
	ChannelGroup,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash, FromPrimitive, ToPrimitive)]
pub enum PluginTargetMode {
	/// Send to all clients in the current channel.
	CurrentChannel,
	/// Send to all clients on the server.
	Server,
	/// Send to all given clients ids.
	Client,
	/// Send to all given clients which are subscribed to the current channel
	/// (i.e. which see the this client).
	CurrentChannelSubsribedClients,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash, FromPrimitive, ToPrimitive)]
pub enum LogLevel {
	/// Everything that is really bad.
	Error = 1,
	/// Everything that might be bad.
	Warning,
	/// Output that might help find a problem.
	Debug,
	/// Informational output.
	Info,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub enum MaxClients {
	Unlimited,
	Inherited,
	Limited(u16),
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct TalkPowerRequest {
	pub time: DateTime<Utc>,
	pub message: String,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Invoker {
	pub name: String,
	pub id: ClientId,
	pub uid: Option<Uid>,
}

#[derive(Debug, PartialEq, Eq, Clone)]
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
