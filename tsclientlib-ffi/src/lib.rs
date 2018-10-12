extern crate chashmap;
#[macro_use]
extern crate failure;
#[macro_use]
extern crate lazy_static;
extern crate num;
extern crate tokio;
extern crate tsclientlib;

use std::ffi::{CStr, CString};
use std::fmt;
use std::os::raw::c_char;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use chashmap::CHashMap;
use num::ToPrimitive;
use tokio::prelude::{future, Future};
use tsclientlib::{ChannelId, ClientId, ConnectOptions, Connection,
	ServerGroupId};

type Result<T> = std::result::Result<T, tsclientlib::Error>;

/// The sender will block when this amount of events is stored in the queue.
const EVENT_CHANNEL_SIZE: usize = 5;

lazy_static! {
	static ref RUNTIME: tokio::runtime::Runtime = tokio::runtime::Runtime::new()
		.unwrap();
	// TODO Find the next free connection id thread safe
	static ref NEXT_CON_ID: AtomicUsize = AtomicUsize::new(0);
	static ref CONNECTIONS: CHashMap<ConnectionId, Connection> = CHashMap::new();

	// TODO It's bad when the sender blocks, maybe use futures
	static ref EVENTS: (std::sync::mpsc::SyncSender<Event>,
		Mutex<std::sync::mpsc::Receiver<Event>>) = {
		let (send, recv) = std::sync::mpsc::sync_channel(EVENT_CHANNEL_SIZE);
		(send, Mutex::new(recv))
	};
}

include!(concat!(env!("OUT_DIR"), "/book_ffi.rs"));

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
#[repr(transparent)]
pub struct ConnectionId(u32);

#[repr(u32)]
pub enum EventType {
	ConnectionAdded,
}

enum Event {
	ConnectionAdded(ConnectionId),
}

#[repr(C)]
pub struct FfiEvent {
	content: FfiEventUnion,
	typ: EventType,
}

#[repr(C)]
pub union FfiEventUnion {
	connection_added: ConnectionId,
}

impl Event {
	fn get_type(&self) -> EventType {
		match self {
			Event::ConnectionAdded(_) => EventType::ConnectionAdded,
		}
	}
}

impl fmt::Display for ConnectionId {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "{:?}", self)
	}
}

trait ConnectionExt {
	fn get_connection(&self) -> &tsclientlib::data::Connection;

	fn get_server(&self) -> &tsclientlib::data::Server;
	fn get_connection_server_data(&self) -> &tsclientlib::data::ConnectionServerData;
	fn get_optional_server_data(&self) -> &tsclientlib::data::OptionalServerData;
	fn get_server_group(&self, id: u64) -> &tsclientlib::data::ServerGroup;

	fn get_client(&self, id: u16) -> &tsclientlib::data::Client;
	fn get_connection_client_data(&self, id: u16) -> &tsclientlib::data::ConnectionClientData;
	fn get_optional_client_data(&self, id: u16) -> &tsclientlib::data::OptionalClientData;

	fn get_channel(&self, id: u64) -> &tsclientlib::data::Channel;
	fn get_optional_channel_data(&self, id: u64) -> &tsclientlib::data::OptionalChannelData;

	fn get_chat_entry(&self, sender_client: u16) -> &tsclientlib::data::ChatEntry;
	fn get_file(&self, id: u64, path: *const c_char, name: *const c_char) -> &tsclientlib::data::File;
}

// TODO Don't unwrap
impl ConnectionExt for tsclientlib::data::Connection {
	fn get_connection(&self) -> &tsclientlib::data::Connection { self }

	fn get_server(&self) -> &tsclientlib::data::Server { &self.server }
	fn get_connection_server_data(&self) -> &tsclientlib::data::ConnectionServerData {
		self.server.connection_data.as_ref().unwrap()
	}
	fn get_optional_server_data(&self) -> &tsclientlib::data::OptionalServerData {
		self.server.optional_data.as_ref().unwrap()
	}
	fn get_server_group(&self, id: u64) -> &tsclientlib::data::ServerGroup {
		self.server.groups.get(&ServerGroupId(id)).unwrap()
	}

	fn get_client(&self, id: u16) -> &tsclientlib::data::Client {
		self.server.clients.get(&ClientId(id)).unwrap()
	}
	fn get_connection_client_data(&self, id: u16) -> &tsclientlib::data::ConnectionClientData {
		self.server.clients.get(&ClientId(id)).unwrap().connection_data.as_ref().unwrap()
	}
	fn get_optional_client_data(&self, id: u16) -> &tsclientlib::data::OptionalClientData {
		self.server.clients.get(&ClientId(id)).unwrap().optional_data.as_ref().unwrap()
	}

	fn get_channel(&self, id: u64) -> &tsclientlib::data::Channel {
		self.server.channels.get(&ChannelId(id)).unwrap()
	}
	fn get_optional_channel_data(&self, id: u64) -> &tsclientlib::data::OptionalChannelData {
		self.server.channels.get(&ChannelId(id)).unwrap().optional_data.as_ref().unwrap()
	}

	fn get_chat_entry(&self, _sender_client: u16) -> &tsclientlib::data::ChatEntry {
		unimplemented!("TODO Chat entries are not implemented")
	}
	fn get_file(&self, _id: u64, _path: *const c_char, _name: *const c_char) -> &tsclientlib::data::File {
		unimplemented!("TODO Files are not implemented")
	}
}

// TODO On future errors, send event

#[no_mangle]
pub extern "C" fn connect(address: *const c_char) -> ConnectionId {
	let address = unsafe { CStr::from_ptr(address) };
	let options = ConnectOptions::new(address.to_str().unwrap());
	let con_id = ConnectionId(NEXT_CON_ID.fetch_add(1, Ordering::Relaxed) as u32);

	RUNTIME.executor().spawn(future::lazy(move ||
		Connection::new(options)
		.map(move |con| {
			// TODO Register handler which removes the connection from the map
			// on disconnects? Or automatically try to reconnect.
			CONNECTIONS.insert(con_id, con);
			EVENTS.0.send(Event::ConnectionAdded(con_id)).unwrap();
		})
	).map_err(|_| ()));
	con_id
}

#[no_mangle]
pub extern "C" fn disconnect(con_id: ConnectionId) {
	RUNTIME.executor().spawn(future::lazy(move ||
		if let Some(con) = CONNECTIONS.get(&con_id) {
			con.clone().disconnect(None)
		} else {
			Box::new(future::err(format_err!("Connection not found").into()))
		}
	).map_err(|_| ()));
}

#[no_mangle]
pub extern "C" fn next_event(ev: *mut FfiEvent) {
	let event = EVENTS.1.lock().unwrap().recv().unwrap();
	unsafe { *ev = FfiEvent {
		content: match &event {
			Event::ConnectionAdded(c) => FfiEventUnion { connection_added: *c },
		},
		typ: event.get_type(),
	} };
}

#[no_mangle]
pub unsafe extern "C" fn free_str(s: *mut c_char) {
	CString::from_raw(s);
}

#[no_mangle]
pub unsafe extern "C" fn free_u64s(ptr: *mut u64, len: usize) {
	Box::from_raw(std::slice::from_raw_parts_mut(ptr, len));
}
