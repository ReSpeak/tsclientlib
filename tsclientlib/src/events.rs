use crate::ClientId;
use crate::data::ServerGroup;

pub enum Events<'a> {
	PropertyChanged { old: Property<'a>, new: Property<'a> },
	PropertyAdded(Property<'a>),
	PropertyRemoved(Property<'a>),
}

// TODO Generate from book
pub enum Property<'a> {
	ClientNickname(&'a str),
	ClientServerGroup(ClientId, &'a ServerGroup),
}
