use std::net::SocketAddr;

use chrono::Duration;

use crate::*;
use crate::data::{ServerGroup, Server, OptionalChannelData, File, Channel,
	OptionalClientData, ConnectionClientData, Client, OptionalServerData,
	ConnectionServerData, ChatEntry, Connection,
};

include!(concat!(env!("OUT_DIR"), "/events.rs"));
// TODO Add connection.get(id: PropertyId)

#[derive(Clone, Debug, PartialEq)]
pub enum Events {
	/// An object of this property was added.
	/// You can get the value of the new object with [`Connection::get`].
	///
	/// [`Connection::get`]: TODO
	PropertyAdded(PropertyId),
	PropertyChanged(PropertyId, Property),
	PropertyRemoved(PropertyId, Property),
}

impl Events {
	pub fn id(&self) -> &PropertyId {
		match self {
			Events::PropertyAdded(id) |
			Events::PropertyChanged(id, _) |
			Events::PropertyRemoved(id, _) => id,
		}
	}
}
