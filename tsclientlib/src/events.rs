use std::net::SocketAddr;

use chrono::Duration;

use crate::*;
use crate::data::{ServerGroup, Server, OptionalChannelData, File, Channel,
	OptionalClientData, ConnectionClientData, Client, OptionalServerData,
	ConnectionServerData, ChatEntry, Connection,
};

include!(concat!(env!("OUT_DIR"), "/events.rs"));

/// An event gets fired when something in the data structure of a connection
/// changes.
///
/// The three different types are
///
/// - [`PropertyAdded`]: When a new item is added, like a client gets assigned
///   a new server group or a new client joins the server.
/// - [`PropertyChanged`]: When an attribute gets updated, e.g. a client changes
///   its nickname or switches to another channel.
/// - [`PropertyRemoved`]: When an item is removed from the data structure. This
///   happens when a client leaves the server (including our own client) or a
///   channel is removed.
///
/// [`PropertyAdded`]: #variant.PropertyAdded
/// [`PropertyChanged`]: #variant.PropertyChanged
/// [`PropertyRemoved`]: #variant.PropertyRemoved
#[derive(Clone, Debug, PartialEq)]
pub enum Events {
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
}

impl Events {
	/// Get the id of the affected property.
	///
	/// For a removed object, you can no longer access it in the connection data
	/// structure but the object is available in the second tuple item.
	pub fn id(&self) -> &PropertyId {
		match self {
			Events::PropertyAdded(id) |
			Events::PropertyChanged(id, _) |
			Events::PropertyRemoved(id, _) => id,
		}
	}
}
