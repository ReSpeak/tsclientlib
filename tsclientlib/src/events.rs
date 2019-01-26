use std::net::SocketAddr;

use chrono::Duration;

use crate::*;
use crate::data::{ServerGroup, Server, OptionalChannelData, File, Channel,
	OptionalClientData, ConnectionClientData, Client, OptionalServerData,
	ConnectionServerData, ChatEntry, Connection,
};

include!(concat!(env!("OUT_DIR"), "/events.rs"));

/// An event gets fired when something in the data structure of a connection
/// changes or something happens like we receive a text message or get poked.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
	/// The object with this id was added.
	///
	/// You can find the new object inside the connection data structure.
	///
	/// Like a client gets assigned a new server group or a new client joins the
	/// server.
	PropertyAdded(PropertyId),
	/// The attribute with this id has changed.
	///
	/// The second tuple item holds the old value of the changed attribute.
	///
	/// E.g. a client changes its nickname or switches to another channel.
	PropertyChanged(PropertyId, Property),
	/// The object with this id was removed.
	///
	/// The object is not accessible anymore in the connection data structure,
	/// but the second tuple item holds the removed object.
	///
	/// This happens when a client leaves the server (including our own client)
	/// or a channel is removed.
	PropertyRemoved(PropertyId, Property),

	Message {
		/// Where we received the message, in the server or channel chat or
		/// directly from another client.
		from: MessageTarget,
		/// The user who wrote the message.
		invoker: Invoker,
		/// The content of the message.
		message: String,
	},
}
