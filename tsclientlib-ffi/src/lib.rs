extern crate chashmap;
#[macro_use]
extern crate failure;
#[macro_use]
extern crate lazy_static;
extern crate tokio;
extern crate tsclientlib;

use std::ffi::CStr;
use std::fmt;
use std::os::raw::c_char;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use chashmap::CHashMap;
use tokio::prelude::{future, Future};
use tokio::timer::Delay;
use tsclientlib::{ConnectOptions, Connection, DisconnectOptions, Reason};

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
