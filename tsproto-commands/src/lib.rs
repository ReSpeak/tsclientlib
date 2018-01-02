extern crate chrono;
extern crate tsproto;
extern crate num;
#[macro_use]
extern crate num_derive;

pub mod errors;
pub mod permissions;
pub mod structs;

/// A `ConnectionId` identifies a connection from us to a server.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct ConnectionId(u16);

/// A `ClientId` identifies a client which is connected to a server.
///
/// Every client that we see on a server has a `ClientId`, even our own
/// connection.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct ClientId(u16);
/// Describes a client or server uid which is a base64
/// encoded hash or a special reserved name.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Uid(String);

/// The database id of a client.
///
/// This is the id which is saved for a client in the database of one specific
/// server.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct ClientDbId(u64);

/// Identifies a channel on a server.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct ChannelId(u64);

/// Identifies a server group on a server.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct ServerGroupId(u64);

/// Identifies a channel group on a server.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct ChannelGroupId(u64);

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct IconHash(i32);

#[derive(Debug, PartialEq, Eq, Clone, Copy, FromPrimitive)]
pub enum TextMessageTargetMode {
	Client,
	Channel,
	Server,
	Max,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, FromPrimitive)]
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

#[derive(Debug, PartialEq, Eq, Clone, Copy, FromPrimitive)]
pub enum HostBannerMode {
	/// Do not adjust
	NoAdjust,
	/// Do not adjust
	AdjustIgnoreAspect,
	/// Do not adjust
	AdjustKeepAspect,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, FromPrimitive)]
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

#[derive(Debug, PartialEq, Eq, Clone, Copy, FromPrimitive)]
pub enum CodecEncryptionMode {
	PerChannel,
	ForcedOff,
	ForcedOn,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, FromPrimitive)]
pub enum MoveReason {
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

#[derive(Debug, PartialEq, Eq, Clone, Copy, FromPrimitive)]
pub enum ClientType {
	Normal,
	Query,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, FromPrimitive)]
pub enum GroupNamingMode {
	/// No group name is displayed.
	None,
	/// Group name is displayed before the client name.
	Before,
	/// Group name is displayed after the client name.
	After,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, FromPrimitive)]
pub enum PermissionGroupDatabaseType {
	/// Template group (used for new virtual servers).
	Template,
	/// Regular group (used for regular clients).
	Regular,
	/// Global query group (used for server query clients).
	Query,
}
