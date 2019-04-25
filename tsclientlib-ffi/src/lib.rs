use std::ffi::{CStr, CString};
use std::{fmt, mem};
use std::os::raw::c_char;
use std::sync::atomic::{AtomicU64, Ordering};

use chashmap::CHashMap;
use crossbeam::channel;
use derive_more::From;
use failure::{format_err, Fail, ResultExt};
use ffigen_derive::FfiGen;
use futures::future::Either;
use futures::sync::oneshot;
use futures::{future, Future};
#[cfg(feature = "audio")]
use futures::{Async, AsyncSink, Poll, Sink, StartSend};
use lazy_static::lazy_static;
use num::{FromPrimitive, ToPrimitive};
use num_derive::{FromPrimitive, ToPrimitive};
#[cfg(feature = "audio")]
use parking_lot::RwLock;
use slog::{error, o, Drain, Logger};
use tsclientlib::events::Event as LibEvent;
use tsclientlib::{
	ChannelId, ClientId, ConnectOptions, Connection, ServerGroupId,
};
#[cfg(feature = "audio")]
use tsproto::packets::OutPacket;
#[cfg(feature = "audio")]
use tsproto_audio::{audio_to_ts, ts_to_audio};

type Result<T> = std::result::Result<T, Error>;
type BoxFuture<T> = Box<Future<Item = T, Error = Error> + Send + 'static>;

pub mod events;
mod ffi_utils;
pub mod special_types;

use events::*;
use ffi_utils::*;
use special_types::*;

// ConnectionId and FutureHandle are an u64, therefore we can expect that they
// will never overflow.
// On a 5 GHz CPU with 1000 cores it would take 1.2 years to iterate through
// all numbers of an u64 (under the assumption that one increment takes one
// cycle).

/// To obtain a unique id for a future, we just increment this counter.
///
/// Use `FutureHandle::next_free` to obtain a handle.
///
///
/// Motivation when using an u32 instead of an u64:
/// In theory, this could lead to two futures using the same counter if we
/// wrap arount in between. We ignore this case because futures are likely
/// short lived and a user does not spawn 4 billion futures while another
/// future is still running.
static NEXT_FUTURE_HANDLE: AtomicU64 = AtomicU64::new(0);

/// Same as `NEXT_FUTURE_HANDLE`.
static NEXT_CONNECTION_ID: AtomicU64 = AtomicU64::new(0);

lazy_static! {
	static ref LOGGER: Logger = {
		// Hook the logger to initialize the panic handler
		color_backtrace::install();

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
	/// A list of all connections that were started but are not yet connected.
	///
	/// By sending a message to the channel, the connection can be canceled.
	static ref CONNECTING: CHashMap<ConnectionId, oneshot::Sender<()>> =
		CHashMap::new();
	/// All currently open connections.
	static ref CONNECTIONS: CHashMap<ConnectionId, Connection> =
		CHashMap::new();

	/// Transfer events to whoever is listening on the `next_event` method.
	static ref EVENTS: (channel::Sender<Event>, channel::Receiver<Event>) =
		channel::unbounded();
}

#[cfg(feature = "audio")]
lazy_static! {
	// TODO In theory, this should be only one for all connections
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
}

include!(concat!(env!("OUT_DIR"), "/book_ffi.rs"));

// **** FFI types ****

#[repr(C)]
pub struct FfiResult {
	/// Often used as `*const c_void`
	pub content: u64,
	pub typ: FfiResultType,
}

#[repr(u8)]
pub enum FfiResultType {
	Ok,
	None,
	Error,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord, Hash, ToPrimitive)]
#[repr(transparent)]
pub struct ConnectionId(u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, ToPrimitive)]
#[repr(transparent)]
pub struct FutureHandle(u64);

#[repr(u8)]
#[derive(Clone, Copy, Debug)]
pub enum FfiEventType {
	ConnectionAdded,
	ConnectionRemoved,
	FutureFinished,
	Message,
	PropertyAdded,
	PropertyChanged,
	PropertyRemoved,
}

#[repr(C)]
pub struct FfiEvent {
	typ: FfiEventType,
	has_connection_id: bool,
	connection: ConnectionId,
	content: FfiEventUnion,
}

#[repr(C)]
pub union FfiEventUnion {
	empty: (),
	future_result: FfiFutureResult,
	message: EventMsg,
	/// Property added or removed
	property: FfiProperty,
	/// Old and new property
	property_changed: FfiPropertyChanged,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct FfiFutureResult {
	/// Is `null` if the future was successful.
	error: *mut c_char,
	handle: FutureHandle,
}
unsafe impl Send for FfiFutureResult {}

#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct EventMsg {
	message: *mut c_char,
	invoker: FfiInvoker,
	target_type: MessageTarget,
}
unsafe impl Send for EventMsg {}

#[derive(Clone, Copy, Debug, ToPrimitive)]
#[repr(u8)]
pub enum MessageTarget {
	Server,
	Channel,
	Client,
	Poke,
}

// **** Non FFI types ****

#[derive(FfiGen)]
pub struct NewEvent {
	con_id: Option<ConnectionId>,
	content: EventContent,
}

#[derive(FfiGen)]
pub enum EventContent {
	ConnectionAdded,
	ConnectionRemoved,
	FutureFinished { handle: FutureHandle, error: Option<String> },
	Message {
		message: String,
		invoker: NewFfiInvoker,
		target: MessageTarget,
	},
	PropertyAdded {
		placeholder: u32,
	},
	PropertyRemoved {
		placeholder: u32,
	},
	PropertyChanged {
		placeholder: u32,
	},
}

#[derive(FfiGen)]
pub struct NewFfiInvoker {
	name: String,
	uid: Option<String>,
	id: u16,
}

enum Event {
	ConnectionAdded(ConnectionId),
	ConnectionRemoved(ConnectionId),
	FutureFinished(FutureHandle, Result<()>),
	Event(ConnectionId, LibEvent),
	/// Used for property lib events, which have to be converted before sending.
	FfiEvent(ConnectionId, FfiEventType, FfiEventUnion),
}

#[derive(Fail, Debug, From)]
enum Error {
	#[fail(display = "{}", _0)]
	Tsclientlib(#[cause] tsclientlib::Error),
	#[fail(display = "{}", _0)]
	Utf8(#[cause] std::str::Utf8Error),
	#[fail(display = "Connection not found")]
	ConnectionNotFound,
	#[fail(display = "Future canceled")]
	Canceled,

	#[fail(display = "{}", _0)]
	Other(#[cause] failure::Compat<failure::Error>),
}

// **** End types ****

impl From<failure::Error> for Error {
	fn from(e: failure::Error) -> Self {
		let r: std::result::Result<(), _> = Err(e);
		Error::Other(r.compat().unwrap_err())
	}
}

impl From<MessageTarget> for u64 {
	fn from(t: MessageTarget) -> u64 {
		t.to_u64().unwrap()
	}
}

impl From<ConnectionId> for u64 {
	fn from(t: ConnectionId) -> u64 {
		t.to_u64().unwrap()
	}
}

impl From<FutureHandle> for u64 {
	fn from(t: FutureHandle) -> u64 {
		t.to_u64().unwrap()
	}
}

impl Event {
	fn get_type(&self) -> FfiEventType {
		match self {
			Event::ConnectionAdded(_) => FfiEventType::ConnectionAdded,
			Event::ConnectionRemoved(_) => FfiEventType::ConnectionRemoved,
			Event::FutureFinished(_, _) => FfiEventType::FutureFinished,
			Event::Event(_, LibEvent::Message { .. }) => FfiEventType::Message,
			Event::Event(_, LibEvent::PropertyAdded { .. }) => {
				FfiEventType::PropertyAdded
			}
			Event::Event(_, LibEvent::PropertyChanged { .. }) => {
				FfiEventType::PropertyChanged
			}
			Event::Event(_, LibEvent::PropertyRemoved { .. }) => {
				FfiEventType::PropertyRemoved
			}
			Event::Event(_, LibEvent::__NonExhaustive) => {
				panic!("Non exhaustive should not be created")
			}
			Event::FfiEvent(_, t, _) => *t,
		}
	}

	fn get_id(&self) -> Option<ConnectionId> {
		match self {
			Event::ConnectionAdded(id) => Some(*id),
			Event::ConnectionRemoved(id) => Some(*id),
			Event::FutureFinished(_, _) => None,
			Event::Event(id, _) => Some(*id),
			Event::FfiEvent(id, _, _) => Some(*id),
		}
	}
}

impl Into<FfiEvent> for Event {
	fn into(self) -> FfiEvent {
		let typ = self.get_type();
		let id = self.get_id();
		FfiEvent {
			content: match self {
				Event::FutureFinished(handle, r) => FfiEventUnion {
					future_result: FfiFutureResult {
						error: r.ffi(),
						handle,
					},
				},
				Event::Event(
					_,
					LibEvent::Message {
						from,
						invoker,
						message,
					},
				) => FfiEventUnion {
					message: EventMsg {
						target_type: from.into(),
						invoker: invoker.ffi(),
						message: message.ffi(),
					},
				},
				Event::Event(
					_,
					LibEvent::PropertyRemoved { id, old, invoker },
				) => {
					let old_ref = old.as_ref();
					FfiEventUnion {
						property: FfiProperty {
							value: old_ref.prop_ffi(),
							invoker: invoker
								.map(|i| i.ffi())
								.unwrap_or_else(Default::default),
							p_type: id.prop_type(),
							id: id.prop_ffi(),
							value_exists: !old_ref.is_none(),
						},
					}
				}
				Event::Event(_, _) => unimplemented!(
					"Property events have to be converted before"
				),
				Event::FfiEvent(_, _, f) => f,
				_ => FfiEventUnion { empty: () },
			},
			typ,
			connection: id.unwrap_or(ConnectionId(0)),
			has_connection_id: id.is_some(),
		}
	}
}

trait LibEventExt {
	fn into_event(
		self,
		con_id: ConnectionId,
		con: &tsclientlib::data::Connection,
	) -> Event;
}

impl LibEventExt for LibEvent {
	/// Used to convert into an `FfiEvent` where a connection is needed to fetch
	/// further information.
	fn into_event(
		self,
		con_id: ConnectionId,
		con: &tsclientlib::data::Connection,
	) -> Event
	{
		let typ;
		let event = match self {
			LibEvent::PropertyAdded { id, invoker } => {
				typ = FfiEventType::PropertyAdded;
				let new_ref = con
					.get_property(&id)
					.expect("Failed to get value of property");
				FfiEventUnion {
					property: FfiProperty {
						value: new_ref.prop_ffi(),
						invoker: invoker
							.map(|i| i.ffi())
							.unwrap_or_else(Default::default),
						p_type: id.prop_type(),
						id: id.prop_ffi(),
						value_exists: !new_ref.is_none(),
					},
				}
			}
			LibEvent::PropertyChanged { id, old, invoker } => {
				typ = FfiEventType::PropertyChanged;
				let new_ref = con
					.get_property(&id)
					.expect("Failed to get value of property");
				let old_ref = old.as_ref();
				FfiEventUnion {
					property_changed: FfiPropertyChanged {
						old: old_ref.prop_ffi(),
						new: new_ref.prop_ffi(),
						invoker: invoker
							.map(|i| i.ffi())
							.unwrap_or_else(Default::default),
						p_type: id.prop_type(),
						id: id.prop_ffi(),
						old_exists: !old_ref.is_none(),
						new_exists: !new_ref.is_none(),
					},
				}
			}
			LibEvent::PropertyRemoved { id, old, invoker } => {
				typ = FfiEventType::PropertyRemoved;
				let old_ref = old.as_ref();
				FfiEventUnion {
					property: FfiProperty {
						value: old_ref.prop_ffi(),
						invoker: invoker
							.map(|i| i.ffi())
							.unwrap_or_else(Default::default),
						p_type: id.prop_type(),
						id: id.prop_ffi(),
						value_exists: !old_ref.is_none(),
					},
				}
			}
			_ => panic!("Unsupported conversion"),
		};

		Event::FfiEvent(con_id, typ, event)
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
#[cfg(feature = "audio")]
struct CurrentAudioSink;
#[cfg(feature = "audio")]
impl Sink for CurrentAudioSink {
	type SinkItem = OutPacket;
	type SinkError = tsclientlib::Error;

	fn start_send(
		&mut self,
		item: Self::SinkItem,
	) -> StartSend<Self::SinkItem, Self::SinkError>
	{
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

#[cfg(feature = "audio")]
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
	fn get_mut_client(
		&self,
		id: u16,
	) -> Option<tsclientlib::data::ClientMut<'a>>;
	fn get_mut_channel(
		&self,
		id: u64,
	) -> Option<tsclientlib::data::ChannelMut<'a>>;
}

impl<'a> ConnectionMutExt<'a> for tsclientlib::data::ConnectionMut<'a> {
	fn get_mut_client(
		&self,
		id: u16,
	) -> Option<tsclientlib::data::ClientMut<'a>>
	{
		self.get_server().get_client(&ClientId(id))
	}
	fn get_mut_channel(
		&self,
		id: u64,
	) -> Option<tsclientlib::data::ChannelMut<'a>>
	{
		self.get_server().get_channel(&ChannelId(id))
	}
}

impl FutureHandle {
	fn next_free() -> Self {
		let id = NEXT_FUTURE_HANDLE.fetch_add(1, Ordering::Relaxed);
		Self(id)
	}
}

impl ConnectionId {
	fn next_free() -> Self {
		let id = NEXT_CONNECTION_ID.fetch_add(1, Ordering::Relaxed);
		Self(id)
	}
}

fn remove_connection(con_id: ConnectionId) {
	#[cfg(feature = "audio")]
	{
		// Disable sound for this connection
		let mut cas = CURRENT_AUDIO_SINK.lock();
		if let Some((id, _)) = &*cas {
			if *id == con_id {
				*cas = None;
			}
		}
		drop(cas);
		T2A_PIPES.remove(&con_id);
	}

	let removed;
	// Cancel connection if it is connecting but not yet ready
	// Important: Test first CONNECTING and then CONNECTIONS because they get
	// first inserted into CONNECTIONS and then removed from CONNECTING so if a
	// connetcion is not in CONNECTING, it has to be in CONNECTIONS.
	if let Some(con) = CONNECTING.remove(&con_id) {
		con.send(()).unwrap();
		removed = true;
	} else {
		// Connection does not exist if it is not in CONNECTIONS
		removed = CONNECTIONS.remove(&con_id).is_some();
	}

	if removed {
		EVENTS.0.send(Event::ConnectionRemoved(con_id)).unwrap();
	}
}

#[no_mangle]
pub unsafe extern "C" fn tscl_connect(
	address: *const c_char,
	con_id: *mut ConnectionId,
) -> FutureHandle
{
	let res = connect(ffi_to_str(&address).unwrap());
	*con_id = res.0;
	res.1.fut_ffi()
}

fn connect(
	address: &str,
) -> (ConnectionId, impl Future<Item = (), Error = Error>) {
	let options = ConnectOptions::new(address).logger(LOGGER.clone());
	let (send, recv) = oneshot::channel();

	let con_id = ConnectionId::next_free();

	// Insert into CONNECTING so it can be canceled
	CONNECTING.insert(con_id, send);

	// TODO Send the connection added event when the user can request the connection
	// status.
	//EVENTS.0.send(Event::ConnectionAdded(con_id)).unwrap();

	let con_id2 = con_id;
	(
		con_id,
		future::lazy(move || {
		#[cfg(feature = "audio")]
		let mut options = options;
		#[cfg(feature = "audio")] {
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
		}
		Connection::new(options).map(move |con| {
			// Or automatically try to reconnect (in tsclientlib)
			con.add_on_disconnect(Box::new(move || {
				remove_connection(con_id);
			}));

			con.add_on_event("tsclientlibffi".into(), Box::new(move |con, events| {
				// TODO Send all at once? Remember the problem with getting
				// ServerGroups one by one, so you never know when they are
				// complete.
				for e in events {
					if let LibEvent::Message { .. } = e {
						EVENTS.0.send(Event::Event(con_id, e.clone())).unwrap();
					} else {
						EVENTS.0.send(e.clone().into_event(con_id, &**con)).unwrap();
					}
				}
			}));

			#[cfg(feature = "audio")] {
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
			}

			CONNECTIONS.insert(con_id, con);
			EVENTS.0.send(Event::ConnectionAdded(con_id)).unwrap();
		})
		.map_err(move |e| {
			error!(LOGGER, "Failed to connect"; "error" => %e);
			remove_connection(con_id);
			e
		})
	})
	// Cancel
	.select2(recv)
	.then(move |r| {
		// Remove from CONNECTING if it is still in there
		CONNECTING.remove(&con_id2);
		match r {
			Ok(_) => Ok(()),
			Err(Either::A((e, _))) => Err(e.into()),
			Err(Either::B(_)) => Err(Error::Canceled),
		}
	}),
	)
}

#[no_mangle]
pub extern "C" fn tscl_disconnect(con_id: ConnectionId) -> FutureHandle {
	disconnect(con_id).fut_ffi()
}

fn disconnect(con_id: ConnectionId) -> BoxFuture<()> {
	// Cancel connection if it is connecting but not yet ready
	if let Some(con) = CONNECTING.remove(&con_id) {
		con.send(()).unwrap();
		return Box::new(future::ok(()));
	}

	if let Some(con) = CONNECTIONS.get(&con_id) {
		Box::new(future::lazy(move || con.disconnect(None).from_err()))
	} else {
		Box::new(future::err(Error::ConnectionNotFound))
	}
}

#[no_mangle]
pub extern "C" fn tscl_is_talking() -> bool {
	#[cfg(feature = "audio")]
	{
		let a2t_pipe = A2T_PIPE.read();
		if let Some(a2t_pipe) = &*a2t_pipe {
			if let Ok(true) = a2t_pipe.is_playing() {
				return true;
			}
		}
	}
	false
}

#[no_mangle]
pub extern "C" fn tscl_set_talking(_talking: bool) {
	#[cfg(feature = "audio")]
	{
		let a2t_pipe = A2T_PIPE.read();
		if let Some(a2t_pipe) = &*a2t_pipe {
			if let Err(e) = a2t_pipe.set_playing(_talking) {
				error!(LOGGER, "Failed to set talking state"; "error" => ?e);
			}
		}
	}
}

#[no_mangle]
pub unsafe extern "C" fn tscl_next_event(ev: *mut FfiEvent) {
	let event = EVENTS.1.recv().unwrap();
	*ev = event.into();
}

/// Send a chat message.
///
/// For the targets `Server` and `Channel`, the `target` parameter is ignored.
#[no_mangle]
pub unsafe extern "C" fn tscl_send_message(
	con_id: ConnectionId,
	target_type: MessageTarget,
	target: u16,
	msg: *const c_char,
) -> FutureHandle
{
	let msg = ffi_to_str(&msg).unwrap();
	send_message(con_id, target_type, target, msg).fut_ffi()
}

fn send_message(
	con_id: ConnectionId,
	target_type: MessageTarget,
	target: u16,
	msg: &str,
) -> BoxFuture<()>
{
	use tsclientlib::MessageTarget as MT;

	if let Some(con) = CONNECTIONS.get(&con_id) {
		let target = match target_type {
			MessageTarget::Server => MT::Server,
			MessageTarget::Channel => MT::Channel,
			MessageTarget::Client => MT::Client(ClientId(target)),
			MessageTarget::Poke => MT::Poke(ClientId(target)),
		};
		Box::new(con.lock().to_mut().send_message(target, msg).from_err())
	} else {
		Box::new(future::err(Error::ConnectionNotFound))
	}
}

#[no_mangle]
pub unsafe extern "C" fn tscl_free_str(ptr: *mut c_char) {
	//println!("Free {:?}", ptr);
	if !ptr.is_null() {
		CString::from_raw(ptr);
	}
}

#[no_mangle]
pub unsafe extern "C" fn tscl_free_u16s(ptr: *mut u16, len: usize) {
	//println!("Free {:?} Len {}", ptr, len);
	Box::from_raw(std::slice::from_raw_parts_mut(ptr, len));
}

#[no_mangle]
pub unsafe extern "C" fn tscl_free_u64s(ptr: *mut u64, len: usize) {
	//println!("Free {:?} Len {}", ptr, len);
	Box::from_raw(std::slice::from_raw_parts_mut(ptr, len));
}

#[no_mangle]
pub unsafe extern "C" fn tscl_free_char_ptrs(
	ptr: *mut *mut c_char,
	len: usize,
)
{
	//println!("Free {:?} Len {}", ptr, len);
	let slice = Box::from_raw(std::slice::from_raw_parts_mut(ptr, len));
	for ptr in &*slice {
		tscl_free_str(*ptr);
	}
}

#[no_mangle]
pub unsafe extern "C" fn tscl_check_interface(name: *const c_char) -> usize {
	let name = ffi_to_str(&name).unwrap();
	match name {
		"FfiEventType" => mem::size_of::<FfiEventType>(),
		"FfiEvent" => mem::size_of::<FfiEvent>(),
		"FfiEventUnion" => mem::size_of::<FfiEventUnion>(),
		"FfiFutureResult" => mem::size_of::<FfiFutureResult>(),
		"FfiEventMsg" => mem::size_of::<EventMsg>(),
		"FfiMessageTarget" => mem::size_of::<MessageTarget>(),
		"FfiInvoker" => mem::size_of::<FfiInvoker>(),
		"MaxClients" => mem::size_of::<FfiMaxClients>(),
		"MaxClientsKind" => mem::size_of::<FfiMaxClientsKind>(),
		"FfiTalkPowerRequest" => mem::size_of::<FfiTalkPowerRequest>(),
		"ConnectionId" => mem::size_of::<ConnectionId>(),
		"FutureResult" => mem::size_of::<FutureHandle>(),
		"FfiProperty" => mem::size_of::<FfiProperty>(),
		"FfiPropertyChanged" => mem::size_of::<FfiPropertyChanged>(),
		"FfiPropertyId" => mem::size_of::<FfiPropertyId>(),
		"FfiPropertyValue" => mem::size_of::<FfiPropertyValue>(),
		"U16U64" => mem::size_of::<U16U64>(),
		"U64StrStr" => mem::size_of::<U64StrStr>(),
		_ => std::usize::MAX,
	}
}
