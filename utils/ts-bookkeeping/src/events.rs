use serde::{Deserialize, Serialize};
use time::{Duration, OffsetDateTime};
use tsproto_types::crypto::EccKeyPubP256;

use crate::data::{
	Channel, ChannelGroup, Client, Connection, ConnectionClientData, ConnectionServerData,
	OptionalChannelData, OptionalClientData, OptionalServerData, Server, ServerGroup,
};
use crate::*;

include!(concat!(env!("OUT_DIR"), "/events.rs"));

/// Additional data for some events.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct ExtraInfo {
	/// Set for e.g. new clients to distinguish if they joined or are made available because we
	/// subscribed to a channel.
	pub reason: Option<Reason>,
}

/// An event gets fired when something in the data structure of a connection
/// changes or something happens like we receive a text message or get poked.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub enum Event {
	/// The object with this id was added.
	///
	/// You can find the new object inside the connection data structure.
	///
	/// Like a client gets assigned a new server group or a new client joins the
	/// server.
	PropertyAdded { id: PropertyId, invoker: Option<Invoker>, extra: ExtraInfo },
	/// The attribute with this id has changed.
	///
	/// The second tuple item holds the old value of the changed attribute.
	///
	/// E.g. a client changes its nickname or switches to another channel.
	PropertyChanged {
		id: PropertyId,
		old: PropertyValue,
		invoker: Option<Invoker>,
		extra: ExtraInfo,
	},
	/// The object with this id was removed.
	///
	/// The object is not accessible anymore in the connection data structure,
	/// but the second tuple item holds the removed object.
	///
	/// This happens when a client leaves the server (including our own client)
	/// or a channel is removed.
	PropertyRemoved {
		id: PropertyId,
		old: PropertyValue,
		invoker: Option<Invoker>,
		extra: ExtraInfo,
	},

	Message {
		/// Where this message was sent to, in the server or channel chat or
		/// directly to client.
		///
		/// This is our own client for private messages from others.
		target: MessageTarget,
		/// The user who wrote the message.
		invoker: Invoker,
		/// The content of the message.
		message: String,
	},
}

impl Event {
	pub fn get_invoker(&self) -> Option<&Invoker> {
		match self {
			Event::PropertyAdded { invoker, .. }
			| Event::PropertyChanged { invoker, .. }
			| Event::PropertyRemoved { invoker, .. } => invoker.as_ref(),
			Event::Message { invoker, .. } => Some(invoker),
		}
	}
}
