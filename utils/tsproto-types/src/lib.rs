use std::fmt;
use std::u64;

use bitflags::bitflags;
use num_derive::{FromPrimitive, ToPrimitive};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

pub mod errors;
pub mod versions;

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

#[derive(
	Clone, Copy, Debug, Deserialize, Eq, FromPrimitive, Hash, PartialEq, Serialize, ToPrimitive
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

bitflags! {
	/// Hints if the client has the permission to make specific actions.
	#[derive(Deserialize, Serialize)]
	pub struct ChannelPermissionHint: u16 {
		/// b_channel_join_*
		const JOIN = 1 << 0;
		/// i_channel_modify_power
		const MODIFY = 1 << 1;
		/// b_channel_delete_flag_force
		const FORCE_DELETE = 1 << 2;
		/// b_channel_delete_*
		const DELETE = 1 << 3;
		/// i_channel_subscribe_power
		const SUBSCRIBE = 1 << 4;
		/// i_channel_description_view_power
		const VIEW_DESCRIPTION = 1 << 5;
		/// i_ft_file_upload_power
		const FILE_UPLOAD = 1 << 6;
		/// i_ft_needed_file_download_power
		const FILE_DOWNLOAD = 1 << 7;
		/// i_ft_file_delete_power
		const FILE_DELETE = 1 << 8;
		/// i_ft_file_rename_power
		const FILE_RENAME = 1 << 9;
		/// i_ft_file_browse_power
		const FILE_BROWSE = 1 << 10;
		/// i_ft_directory_create_power
		const FILE_DIRECTORY_CREATE = 1 << 11;
		/// i_channel_permission_modify_power
		const MODIFY_PERMISSIONS = 1 << 12;
	}
}

bitflags! {
	/// Hints if the client has the permission to make specific actions.
	#[derive(Deserialize, Serialize)]
	pub struct ClientPermissionHint: u16 {
		/// i_client_kick_from_server_power
		const KICK_SERVER = 1 << 0;
		/// i_client_kick_from_channel_power
		const KICK_CHANNEL = 1 << 1;
		/// i_client_ban_power
		const BAN = 1 << 2;
		/// i_client_move_power
		const MOVE_CLIENT = 1 << 3;
		/// i_client_private_textmessage_power
		const PRIVATE_MESSAGE = 1 << 4;
		/// i_client_poke_power
		const POKE = 1 << 5;
		/// i_client_whisper_power
		const WHISPER = 1 << 6;
		/// i_client_complain_power
		const COMPLAIN = 1 << 7;
		/// i_client_permission_modify_power
		const MODIFY_PERMISSIONS = 1 << 8;
	}
}

#[derive(
	Clone, Copy, Debug, Deserialize, Eq, FromPrimitive, Hash, PartialEq, Serialize, ToPrimitive
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
	Clone, Copy, Debug, Deserialize, Eq, FromPrimitive, Hash, PartialEq, Serialize, ToPrimitive
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
	Clone, Copy, Debug, Deserialize, Eq, FromPrimitive, Hash, PartialEq, Serialize, ToPrimitive
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
	Clone, Copy, Debug, Deserialize, Eq, FromPrimitive, Hash, PartialEq, Serialize, ToPrimitive
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
	Clone, Copy, Debug, Deserialize, Eq, FromPrimitive, Hash, PartialEq, Serialize, ToPrimitive
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
	Clone, Copy, Debug, Deserialize, Eq, FromPrimitive, Hash, PartialEq, Serialize, ToPrimitive
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
	Clone, Copy, Debug, Deserialize, Eq, FromPrimitive, Hash, PartialEq, Serialize, ToPrimitive
)]
pub enum ClientType {
	Normal,
	/// Server query client
	Query,
}

#[derive(
	Clone, Copy, Debug, Deserialize, Eq, FromPrimitive, Hash, PartialEq, Serialize, ToPrimitive
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
	Clone, Copy, Debug, Deserialize, Eq, FromPrimitive, Hash, PartialEq, Serialize, ToPrimitive
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
	Clone, Copy, Debug, Deserialize, Eq, FromPrimitive, Hash, PartialEq, Serialize, ToPrimitive
)]
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

#[derive(
	Clone, Copy, Debug, Deserialize, Eq, FromPrimitive, Hash, PartialEq, Serialize, ToPrimitive
)]
pub enum ChannelType {
	Permanent,
	SemiPermanent,
	Temporary,
}

#[derive(
	Clone, Copy, Debug, Deserialize, Eq, FromPrimitive, Hash, PartialEq, Serialize, ToPrimitive
)]
pub enum TokenType {
	/// Server group token (`id1={groupId}, id2=0`)
	ServerGroup,
	/// Channel group token (`id1={groupId}, id2={channelId}`)
	ChannelGroup,
}

#[derive(
	Clone, Copy, Debug, Deserialize, Eq, FromPrimitive, Hash, PartialEq, Serialize, ToPrimitive
)]
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

#[derive(
	Clone, Copy, Debug, Deserialize, Eq, FromPrimitive, Hash, PartialEq, Serialize, ToPrimitive
)]
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
