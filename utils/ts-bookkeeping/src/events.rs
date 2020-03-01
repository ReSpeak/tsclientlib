use std::net::SocketAddr;

use time::{OffsetDateTime, Duration};
use serde::{Deserialize, Serialize};

use crate::data::{
	Channel, ChatEntry, Client, Connection, ConnectionClientData,
	ConnectionServerData, File, OptionalChannelData, OptionalClientData,
	OptionalServerData, Server, ServerGroup,
};
use crate::*;

include!(concat!(env!("OUT_DIR"), "/events.rs"));

/// An event gets fired when something in the data structure of a connection
/// changes or something happens like we receive a text message or get poked.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[non_exhaustive]
pub enum Event {
	/// The object with this id was added.
	///
	/// You can find the new object inside the connection data structure.
	///
	/// Like a client gets assigned a new server group or a new client joins the
	/// server.
	PropertyAdded { id: PropertyId, invoker: Option<Invoker> },
	/// The attribute with this id has changed.
	///
	/// The second tuple item holds the old value of the changed attribute.
	///
	/// E.g. a client changes its nickname or switches to another channel.
	PropertyChanged {
		id: PropertyId,
		old: PropertyValue,
		invoker: Option<Invoker>,
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
	},

	Message {
		/// Where this message was sent to, in the server or channel chat or
		/// directly to client.
		///
		/// This is our own client for messages private messages from others.
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
