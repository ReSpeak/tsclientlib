#[macro_use]
extern crate failure;

use std::ffi::{CStr, CString};
use std::fmt;
use std::os::raw::c_char;

use chashmap::CHashMap;
use crossbeam::channel;
use futures::{future, Future, Sink, StartSend, Async, AsyncSink, Poll};
use lazy_static::lazy_static;
use num::ToPrimitive;
use parking_lot::{Mutex, RwLock};
use slog::{error, o, Drain, Logger};
use tsclientlib::{
	ChannelId, ClientId, ConnectOptions, Connection, ServerGroupId,
};
use tsclientlib::events::Event as LibEvent;
use tsproto::packets::OutPacket;
use tsproto_audio::{audio_to_ts, ts_to_audio};

//type Result<T> = std::result::Result<T, tsclientlib::Error>;

lazy_static! {
	static ref LOGGER: Logger = {
		let decorator = slog_term::TermDecorator::new().build();
		let drain = slog_term::CompactFormat::new(decorator).build().fuse();
		let drain = slog_async::Async::new(drain).build().fuse();

		Logger::root(drain, o!())
	};

	static ref RUNTIME: tokio::runtime::Runtime = tokio::runtime::Builder::new()
		// Limit to two threads
		.core_threads(2)
		.build()
		.unwrap();
	static ref FIRST_FREE_CON_ID: Mutex<ConnectionId> =
		Mutex::new(ConnectionId(0));
	static ref CONNECTIONS: CHashMap<ConnectionId, Connection> =
		CHashMap::new();

	// TODO In theory, this should be only one for every connection
	/// The gstreamer pipeline which plays back other peoples voice.
	static ref T2A_PIPES: CHashMap<ConnectionId, ts_to_audio::Pipeline> =
		CHashMap::new();

	/// The gstreamer pipeline which captures the microphone and sends it to
	/// TeamSpeak.
	static ref A2T_PIPE: RwLock<Option<audio_to_ts::Pipeline>> =
		RwLock::new(None);

	/// The sink for packets where the `A2T_PIPE` will put packets.
	static ref CURRENT_AUDIO_SINK: Mutex<Option<(ConnectionId, Box<
		Sink<SinkItem=OutPacket, SinkError=tsclientlib::Error> + Send>)>> =
		Mutex::new(None);

	/// Transfer events to whoever is listening on the `next_event` method.
	static ref EVENTS: (channel::Sender<Event>, channel::Receiver<Event>) =
		channel::unbounded();
}

include!(concat!(env!("OUT_DIR"), "/book_ffi.rs"));

#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct ConnectionId(u32);

#[repr(u32)]
pub enum EventType {
	ConnectionAdded,
	ConnectionRemoved,
	Message,
}

enum Event {
	ConnectionAdded(ConnectionId),
	ConnectionRemoved(ConnectionId),
	Event(ConnectionId, LibEvent),
}

#[repr(C)]
pub struct FfiEvent {
	content: FfiEventUnion,
	typ: EventType,
}

#[repr(C)]
pub union FfiEventUnion {
	connection_added: ConnectionId,
	connection_removed: ConnectionId,
	message: EventMsg,
}

#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct EventMsg {
	connection: ConnectionId,
	target_type: MessageTarget,
	invoker: u16,
	message: *mut c_char,
}

#[derive(Clone, Copy, Debug)]
#[repr(u32)]
pub enum MessageTarget {
	Server,
	Channel,
	Client,
	Poke,
}

impl Event {
	fn get_type(&self) -> EventType {
		match self {
			Event::ConnectionAdded(_) => EventType::ConnectionAdded,
			Event::ConnectionRemoved(_) => EventType::ConnectionRemoved,
			Event::Event(_, LibEvent::Message { .. }) => EventType::Message,
			Event::Event(_, _) => unimplemented!("Events apart from message are not yet implemented"),
		}
	}
}

impl fmt::Display for ConnectionId {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "{:?}", self)
	}
}

impl From<tsclientlib::MessageTarget> for MessageTarget {
	fn from(t: tsclientlib::MessageTarget) -> Self {
		use tsclientlib::MessageTarget as MT;

		match t {
			MT::Server => MessageTarget::Server,
			MT::Channel => MessageTarget::Channel,
			MT::Client(_) => MessageTarget::Client,
			MT::Poke(_) => MessageTarget::Poke,
		}
	}
}

/// Redirect everything to `CURRENT_AUDIO_SINK`.
struct CurrentAudioSink;
impl Sink for CurrentAudioSink {
	type SinkItem = OutPacket;
	type SinkError = tsclientlib::Error;

	fn start_send(
		&mut self,
		item: Self::SinkItem
	) -> StartSend<Self::SinkItem, Self::SinkError> {
		let mut cas = CURRENT_AUDIO_SINK.lock();
		if let Some(sink) = &mut *cas {
			sink.1.start_send(item)
		} else {
			Ok(AsyncSink::Ready)
		}
	}

	fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
		let mut cas = CURRENT_AUDIO_SINK.lock();
		if let Some(sink) = &mut *cas {
			sink.1.poll_complete()
		} else {
			Ok(Async::Ready(()))
		}
	}

	fn close(&mut self) -> Poll<(), Self::SinkError> {
		let mut cas = CURRENT_AUDIO_SINK.lock();
		if let Some(sink) = &mut *cas {
			sink.1.close()
		} else {
			Ok(Async::Ready(()))
		}
	}
}

impl audio_to_ts::PacketSinkCreator<tsclientlib::Error> for CurrentAudioSink {
	type S = CurrentAudioSink;
	fn get_sink(&self) -> Self::S { CurrentAudioSink }
}

trait ConnectionExt {
	fn get_connection(&self) -> &tsclientlib::data::Connection;

	fn get_server(&self) -> &tsclientlib::data::Server;
	fn get_connection_server_data(
		&self,
	) -> &tsclientlib::data::ConnectionServerData;
	fn get_optional_server_data(
		&self,
	) -> &tsclientlib::data::OptionalServerData;
	fn get_server_group(&self, id: u64) -> &tsclientlib::data::ServerGroup;

	fn get_client(&self, id: u16) -> &tsclientlib::data::Client;
	fn get_connection_client_data(
		&self,
		id: u16,
	) -> &tsclientlib::data::ConnectionClientData;
	fn get_optional_client_data(
		&self,
		id: u16,
	) -> &tsclientlib::data::OptionalClientData;

	fn get_channel(&self, id: u64) -> &tsclientlib::data::Channel;
	fn get_optional_channel_data(
		&self,
		id: u64,
	) -> &tsclientlib::data::OptionalChannelData;

	fn get_chat_entry(
		&self,
		sender_client: u16,
	) -> &tsclientlib::data::ChatEntry;
	fn get_file(
		&self,
		id: u64,
		path: *const c_char,
		name: *const c_char,
	) -> &tsclientlib::data::File;
}

// TODO Don't unwrap
impl ConnectionExt for tsclientlib::data::Connection {
	fn get_connection(&self) -> &tsclientlib::data::Connection { self }

	fn get_server(&self) -> &tsclientlib::data::Server { &self.server }
	fn get_connection_server_data(
		&self,
	) -> &tsclientlib::data::ConnectionServerData {
		self.server.connection_data.as_ref().unwrap()
	}
	fn get_optional_server_data(
		&self,
	) -> &tsclientlib::data::OptionalServerData {
		self.server.optional_data.as_ref().unwrap()
	}
	fn get_server_group(&self, id: u64) -> &tsclientlib::data::ServerGroup {
		self.server.groups.get(&ServerGroupId(id)).unwrap()
	}

	fn get_client(&self, id: u16) -> &tsclientlib::data::Client {
		self.server.clients.get(&ClientId(id)).unwrap()
	}
	fn get_connection_client_data(
		&self,
		id: u16,
	) -> &tsclientlib::data::ConnectionClientData
	{
		self.server
			.clients
			.get(&ClientId(id))
			.unwrap()
			.connection_data
			.as_ref()
			.unwrap()
	}
	fn get_optional_client_data(
		&self,
		id: u16,
	) -> &tsclientlib::data::OptionalClientData
	{
		self.server
			.clients
			.get(&ClientId(id))
			.unwrap()
			.optional_data
			.as_ref()
			.unwrap()
	}

	fn get_channel(&self, id: u64) -> &tsclientlib::data::Channel {
		self.server.channels.get(&ChannelId(id)).unwrap()
	}
	fn get_optional_channel_data(
		&self,
		id: u64,
	) -> &tsclientlib::data::OptionalChannelData
	{
		self.server
			.channels
			.get(&ChannelId(id))
			.unwrap()
			.optional_data
			.as_ref()
			.unwrap()
	}

	fn get_chat_entry(
		&self,
		_sender_client: u16,
	) -> &tsclientlib::data::ChatEntry
	{
		unimplemented!("TODO Chat entries are not implemented")
	}
	fn get_file(
		&self,
		_id: u64,
		_path: *const c_char,
		_name: *const c_char,
	) -> &tsclientlib::data::File
	{
		unimplemented!("TODO Files are not implemented")
	}
}

trait ConnectionMutExt<'a> {
	fn get_mut_client(&self, id: u16) -> Option<tsclientlib::data::ClientMut<'a>>;
	fn get_mut_channel(&self, id: u64) -> Option<tsclientlib::data::ChannelMut<'a>>;
}

impl<'a> ConnectionMutExt<'a> for tsclientlib::data::ConnectionMut<'a> {
	fn get_mut_client(&self, id: u16) -> Option<tsclientlib::data::ClientMut<'a>> {
		self.get_server().get_client(&ClientId(id))
	}
	fn get_mut_channel(&self, id: u64) -> Option<tsclientlib::data::ChannelMut<'a>> {
		self.get_server().get_channel(&ChannelId(id))
	}
}

// TODO On future errors, send event

impl ConnectionId {
	fn next_free() -> Self {
		let mut next_free = FIRST_FREE_CON_ID.lock();
		let res = *next_free;
		let mut next = res.0 + 1;
		while CONNECTIONS.contains_key(&ConnectionId(next)) {
			next += 1;
		}
		*next_free = ConnectionId(next);
		res
	}

	/// Should be called when a connection is removed
	fn mark_free(&self) {
		let mut next_free = FIRST_FREE_CON_ID.lock();
		if *self < *next_free {
			*next_free = *self;
		}
	}
}

fn remove_connection(con_id: ConnectionId) {
	// Disable sound for this connection
	let mut cas = CURRENT_AUDIO_SINK.lock();
	if let Some((id, _)) = &*cas {
		if *id == con_id {
			*cas = None;
		}
	}
	drop(cas);
	T2A_PIPES.remove(&con_id);

	CONNECTIONS.remove(&con_id);
	con_id.mark_free();
	EVENTS.0.send(Event::ConnectionRemoved(con_id)).unwrap();
}

#[no_mangle]
pub extern "C" fn connect(address: *const c_char) -> ConnectionId {
	let address = unsafe { CStr::from_ptr(address) };
	let options = ConnectOptions::new(address.to_str().unwrap())
		.logger(LOGGER.clone());
	let con_id = ConnectionId::next_free();

	RUNTIME.executor().spawn(
		future::lazy(move || {
			let mut options = options;
			// Create TeamSpeak to audio pipeline
			match ts_to_audio::Pipeline::new(LOGGER.clone(),
				RUNTIME.executor()) {
				Ok(t2a_pipe) => {
					let aph = t2a_pipe.create_packet_handler();
					options = options.audio_packet_handler(aph);
					T2A_PIPES.insert(con_id, t2a_pipe);
				}
				Err(e) => error!(LOGGER, "Failed to create t2a pipeline";
					"error" => ?e),
			}
			Connection::new(options).map(move |con| {
				// Or automatically try to reconnect.
				con.add_on_disconnect(Box::new(move || {
					remove_connection(con_id);
				}));

				con.add_on_event("tsclientlibffi".into(), Box::new(move |_, events| {
					// TODO Send all at once? Remember the problem with getting
					// ServerGroups one by one, so you never know when they are
					// complete.
					for e in events {
						EVENTS.0.send(Event::Event(con_id, e.clone())).unwrap();
					}
				}));

				// Create audio to TeamSpeak pipeline
				if A2T_PIPE.read().is_none() {
					let mut a2t_pipe = A2T_PIPE.write();
					if a2t_pipe.is_none() {
						match audio_to_ts::Pipeline::new(LOGGER.clone(),
							CurrentAudioSink, RUNTIME.executor(), None) {
							Ok(pipe) => {
								*a2t_pipe = Some(pipe);
							}
							Err(e) => error!(LOGGER,
								"Failed to create a2t pipeline";
								"error" => ?e),
						}

						// Set new connection as default talking server
						*CURRENT_AUDIO_SINK.lock() =
							Some((con_id, Box::new(con.get_packet_sink())));
					}
				}

				CONNECTIONS.insert(con_id, con);
				EVENTS.0.send(Event::ConnectionAdded(con_id)).unwrap();
			})
			.map_err(move |e| {
				error!(LOGGER, "Failed to connect"; "error" => %e);
				remove_connection(con_id);
			})
		})
	);
	con_id
}

#[no_mangle]
pub extern "C" fn disconnect(con_id: ConnectionId) {
	RUNTIME.executor().spawn(
		future::lazy(move || {
			if let Some(con) = CONNECTIONS.get(&con_id) {
				con.clone().disconnect(None)
			} else {
				Box::new(future::err(
					format_err!("Connection not found").into(),
				))
			}
		})
		.map_err(|_| ()),
	);
}

#[no_mangle]
pub extern "C" fn is_talking() -> bool {
	let a2t_pipe = A2T_PIPE.read();
	if let Some(a2t_pipe) = &*a2t_pipe {
		if let Ok(true) = a2t_pipe.is_playing() {
			return true;
		}
	}
	false
}

#[no_mangle]
pub extern "C" fn set_talking(talking: bool) {
	let a2t_pipe = A2T_PIPE.read();
	if let Some(a2t_pipe) = &*a2t_pipe {
		if let Err(e) = a2t_pipe.set_playing(talking) {
			error!(LOGGER, "Failed to set talking state"; "error" => ?e);
		}
	}
}

#[no_mangle]
pub extern "C" fn next_event(ev: *mut FfiEvent) {
	let event = EVENTS.1.recv().unwrap();
	unsafe {
		let typ = event.get_type();
		*ev = FfiEvent {
			content: match event {
				Event::ConnectionAdded(c) => FfiEventUnion {
					connection_added: c,
				},
				Event::ConnectionRemoved(c) => FfiEventUnion {
					connection_removed: c,
				},
				Event::Event(id, LibEvent::Message { from, invoker, message }) => FfiEventUnion {
					message: EventMsg {
						connection: id,
						target_type: from.into(),
						invoker: invoker.id.0,
						message: CString::new(message).unwrap().into_raw(),
					}
				},
				Event::Event(_, _) => unimplemented!("Events apart from message are not yet implemented"),
			},
			typ,
		}
	};
}

/// Send a chat message.
///
/// For the targets `Server` and `Channel`, the `target` parameter is ignored.
#[no_mangle]
pub extern "C" fn send_message(con_id: ConnectionId, target_type: MessageTarget, target: u16, msg: *const c_char) {
	use tsclientlib::MessageTarget as MT;

	// TODO Returns future
	let msg = unsafe { CStr::from_ptr(msg) };
	let msg = msg.to_str().unwrap();
	if let Some(con) = CONNECTIONS.get(&con_id) {
		let target = match target_type {
			MessageTarget::Server => MT::Server,
			MessageTarget::Channel => MT::Channel,
			MessageTarget::Client => MT::Client(ClientId(target)),
			MessageTarget::Poke => MT::Poke(ClientId(target)),
		};
		RUNTIME.executor().spawn(con.lock().to_mut().send_message(target, &msg)
			.map_err(|e| error!(LOGGER, "Failed to send message"; "error" => ?e)));
	} else {
		error!(LOGGER, "Connection not found"; "function" => "send_message");
	}
}

#[no_mangle]
pub unsafe extern "C" fn free_str(s: *mut c_char) { CString::from_raw(s); }

#[no_mangle]
pub unsafe extern "C" fn free_u64s(ptr: *mut u64, len: usize) {
	Box::from_raw(std::slice::from_raw_parts_mut(ptr, len));
}

#[no_mangle]
pub unsafe extern "C" fn free_u16s(ptr: *mut u16, len: usize) {
	Box::from_raw(std::slice::from_raw_parts_mut(ptr, len));
}

#[no_mangle]
pub unsafe extern "C" fn free_char_ptrs(ptr: *mut *mut c_char, len: usize) {
	Box::from_raw(std::slice::from_raw_parts_mut(ptr, len));
}
