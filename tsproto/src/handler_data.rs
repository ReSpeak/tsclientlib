use std::collections::HashMap;
use std::mem;
use std::net::SocketAddr;
use std::sync::{Arc, Weak};

use bytes::Bytes;
use futures::sync::{mpsc, oneshot};
use futures::{future, Future, Sink, stream, Stream};
use parking_lot::{Mutex, RwLock};
use slog::Drain;
use tokio::codec::BytesCodec;
use tokio::net::{UdpFramed, UdpSocket};
use {slog, slog_async, slog_term, tokio};

use crate::connection::*;
use crate::connectionmanager::ConnectionManager;
use crate::crypto::EccKeyPrivP256;
use crate::packet_codec::{PacketCodecReceiver, PacketCodecSender};
use crate::packets::*;
use crate::resend::DefaultResender;
use crate::{Error, LockedHashMap, Result};

pub type DataM<CM> = Arc<Mutex<Data<CM>>>;

/// A listener for added and removed connections.
pub trait ConnectionListener<CM: ConnectionManager>: Send {
	/// Called when a new connection is created.
	///
	/// Return `false` to remain in the list of listeners.
	/// If `true` is returned, this listener will be removed.
	///
	/// Per default, `false` is returned.
	fn on_connection_created(
		&mut self,
		_key: &mut CM::Key,
		_adata: &mut CM::AssociatedData,
		_con: &mut Connection,
	) -> bool {
		false
	}

	/// Called when a connection is removed.
	///
	/// Return `false` to remain in the list of listeners.
	/// If `true` is returned, this listener will be removed.
	///
	/// Per default, `false` is returned.
	fn on_connection_removed(
		&mut self,
		_key: &CM::Key,
		_con: &mut ConnectionValue<CM::AssociatedData>,
	) -> bool {
		false
	}
}

/// Will be cloned for some of the incoming packets. So be careful with
/// modifying the internal state, it is not global.
pub trait PacketHandler<T: 'static>: Send {
	/// Called for every new connection.
	///
	/// The `command_stream` gets all command and init packets.
	/// The `audio_stream` gets all audio packets.
	fn new_connection<S1, S2, S3, S4>(
		&mut self,
		con_val: &ConnectionValue<T>,
		s2c_init_stream: S1,
		c2s_init_stream: S2,
		command_stream: S3,
		audio_stream: S4,
	) where
		S1: Stream<Item = InS2CInit, Error = Error> + Send + 'static,
		S2: Stream<Item = InC2SInit, Error = Error> + Send + 'static,
		S3: Stream<Item = InCommand, Error = Error> + Send + 'static,
		S4: Stream<Item = InAudio, Error = Error> + Send + 'static;
}

/// The `observe` method is called on every incoming packet.
pub trait InUdpPacketObserver: Send + Sync {
	/// The `observe` method should not take too long, because it holds a lock.
	fn observe(&self, addr: SocketAddr, udp_packet: &InPacket);
}

/// The `observe` method is called on every outgoing packet.
pub trait OutUdpPacketObserver: Send + Sync {
	/// The `observe` method should not take too long, because it holds a lock.
	fn observe(&self, addr: SocketAddr, udp_packet: &[u8]);
}

/// The `observe` method is called on every incoming packet.
pub trait InPacketObserver<T>: Send + Sync {
	/// The `observe` method should not take too long, because it holds a lock.
	fn observe(&self, connection: &mut (T, Connection), packet: &InPacket);
}

/// The `observe` method is called on every outgoing packet.
pub trait OutPacketObserver<T>: Send + Sync {
	/// The `observe` method should not take too long, because it holds a lock.
	/// It is possible to modify the given object, but that should only be done
	/// in rare cases and if you know what you do.
	fn observe(&self, connection: &mut (T, Connection), packet: &mut Packet);
}

/// The `observe` method is called on every incoming packet.
pub trait InCommandObserver<T>: Send + Sync {
	/// The `observe` method should not take too long, because it holds a lock.
	fn observe(&self, connection: &mut (T, Connection), cmd: &InCommand);
}

pub struct ConnectionValue<T: 'static> {
	pub mutex: Arc<Mutex<(T, Connection)>>,
	pub(crate) out_packet_observer:
		LockedHashMap<String, Box<OutPacketObserver<T>>>,
}

impl<T: Send + 'static> ConnectionValue<T> {
	pub(crate) fn new(data: T, con: Connection,
		out_packet_observer: LockedHashMap<String, Box<OutPacketObserver<T>>>) -> Self {
		Self {
			mutex: Arc::new(Mutex::new((data, con))),
			out_packet_observer,
		}
	}

	fn encode_packet(
		&self,
		mut packet: Packet,
	) -> Box<Stream<Item = (PacketType, u16, Bytes), Error = Error> + Send> {
		let mut con = self.mutex.lock();
		let con = &mut *con;
		// Call observer
		for o in self.out_packet_observer.read().values() {
			o.observe(con, &mut packet);
		}

		let codec = PacketCodecSender::new(con.1.is_client);
		let p_type = packet.header.get_type();

		let mut udp_packets = match codec.encode_packet(&mut con.1, packet) {
			Ok(r) => r,
			Err(e) => return Box::new(stream::once(Err(e))),
		};

		let udp_packets = udp_packets
			.drain(..)
			.map(|(p_id, p)| (p_type, p_id, p))
			.collect::<Vec<_>>();
		Box::new(stream::iter_ok(udp_packets))
	}

	pub fn downgrade(&self) -> ConnectionValueWeak<T> {
		ConnectionValueWeak {
			mutex: Arc::downgrade(&self.mutex),
			out_packet_observer: self.out_packet_observer.clone(),
		}
	}
}

impl<T: 'static> PartialEq for ConnectionValue<T> {
	fn eq(&self, other: &Self) -> bool {
		self as *const ConnectionValue<_> == other as *const _
	}
}

impl<T: 'static> Eq for ConnectionValue<T> {}

impl<T: 'static> Clone for ConnectionValue<T> {
	fn clone(&self) -> Self {
		Self {
			mutex: self.mutex.clone(),
			out_packet_observer: self.out_packet_observer.clone(),
		}
	}
}

pub struct ConnectionValueWeak<T: Send + 'static> {
	pub mutex: Weak<Mutex<(T, Connection)>>,
	pub(crate) out_packet_observer: LockedHashMap<String, Box<OutPacketObserver<T>>>,
}

impl<T: Send + 'static> ConnectionValueWeak<T> {
	pub fn upgrade(&self) -> Option<ConnectionValue<T>> {
		self.mutex.upgrade().map(|mutex| ConnectionValue {
			mutex,
			out_packet_observer: self.out_packet_observer.clone(),
		})
	}

	pub fn as_udp_packet_sink(
		&self,
	) -> crate::connection::ConnectionUdpPacketSink<T> {
		crate::connection::ConnectionUdpPacketSink::new(self.clone())
	}

	pub fn as_packet_sink(
		&self,
	) -> impl Sink<SinkItem = Packet, SinkError = Error> {
		let cv = self.clone();
		self.as_udp_packet_sink().with_flat_map(move |p| {
			if let Some(cv) = cv.upgrade() {
				cv.encode_packet(p)
			} else {
				Box::new(stream::once(Err(
					format_err!("Connection is gone").into()
				)))
			}
		})
	}
}

impl<T: Send + 'static> Clone for ConnectionValueWeak<T> {
	fn clone(&self) -> Self {
		Self {
			mutex: self.mutex.clone(),
			out_packet_observer: self.out_packet_observer.clone(),
		}
	}
}

/// The stored data for our server or client.
///
/// This handles a single socket.
///
/// The list of connections is not managed by this struct, but it is passed to
/// an instance of [`ConnectionManager`] that is stored here.
///
/// [`ConnectionManager`]: trait.ConnectionManager.html
pub struct Data<CM: ConnectionManager + 'static> {
	/// If this structure is owned by a client or a server.
	pub is_client: bool,
	/// The address of the socket.
	pub local_addr: SocketAddr,
	/// The private key of this instance.
	pub private_key: EccKeyPrivP256,
	pub logger: slog::Logger,

	/// The sink of udp packets.
	pub udp_packet_sink: mpsc::Sender<(SocketAddr, Bytes)>,
	exit_send: oneshot::Sender<()>,

	/// The default resend config. It gets copied for each new connection.
	resend_config: crate::resend::ResendConfig,

	pub connections:
		LockedHashMap<CM::Key, ConnectionValue<CM::AssociatedData>>,
	pub packet_handler: CM::PacketHandler,

	/// Observe incoming `UdpPacket`s.
	pub(crate) in_udp_packet_observer:
		LockedHashMap<String, Box<InUdpPacketObserver>>,
	/// Observe outgoing `UdpPacket`s.
	pub(crate) out_udp_packet_observer:
		LockedHashMap<String, Box<OutUdpPacketObserver>>,

	/// Observe incoming `Packet`s.
	pub(crate) in_packet_observer:
		LockedHashMap<String, Box<InPacketObserver<CM::AssociatedData>>>,
	/// Observe outgoing `Packet`s.
	pub(crate) out_packet_observer:
		LockedHashMap<String, Box<OutPacketObserver<CM::AssociatedData>>>,

	/// Observe incoming `Commands`s.
	pub(crate) in_command_observer:
		LockedHashMap<String, Box<InCommandObserver<CM::AssociatedData>>>,
	/// Observe outgoing `Commands`s.

	/// A list of all connected clients or servers
	///
	/// You should not add or remove connections directly using the manager,
	/// unless you know what you are doing. E.g. connection listeners are not
	/// called if you do so.
	/// Instead, use the [`add_connection`] und [`remove_connection`] function.
	///
	/// [`add_connection`]: #method.add_connection
	/// [`remove_connection`]: #method.remove_connection
	pub connection_manager: CM,
	/// Listen for new or removed connections.
	pub connection_listeners: Vec<Box<ConnectionListener<CM>>>,
}

impl<CM: ConnectionManager + 'static> Drop for Data<CM> {
	fn drop(&mut self) {
		// Ignore if the receiver was already dropped
		let sender = mem::replace(&mut self.exit_send, oneshot::channel().0);
		let _ = sender.send(());
	}
}

impl<CM: ConnectionManager + 'static> Data<CM> {
	/// An optional logger can be provided. If none is provided, a new one will
	/// be created.
	pub fn new<L: Into<Option<slog::Logger>>>(
		local_addr: SocketAddr,
		private_key: EccKeyPrivP256,
		is_client: bool,
		unknown_udp_packet_sink: Option<mpsc::Sender<(SocketAddr, InPacket)>>,
		packet_handler: CM::PacketHandler,
		connection_manager: CM,
		logger: L,
	) -> Result<Arc<Mutex<Self>>> {
		let logger = logger.into().unwrap_or_else(|| {
			let decorator = slog_term::TermDecorator::new().build();
			let drain = slog_term::FullFormat::new(decorator).build().fuse();
			let drain = slog_async::Async::new(drain).build().fuse();

			slog::Logger::root(drain, o!())
		});

		// Create the socket
		let socket = UdpSocket::bind(&local_addr)?;
		let local_addr = socket.local_addr().unwrap_or(local_addr);
		debug!(logger, "Listening"; "local_addr" => %local_addr);
		let (sink, stream) = UdpFramed::new(socket, BytesCodec::new()).split();

		let (exit_send, exit_recv) = oneshot::channel();
		let (udp_packet_sink, udp_packet_sink_sender) =
			mpsc::channel(crate::UDP_SINK_CAPACITY);
		let logger2 = logger.clone();

		let connections = Arc::new(RwLock::new(HashMap::new()));
		let in_udp_packet_observer = Arc::new(RwLock::new(HashMap::new()));
		let out_udp_packet_observer = Arc::new(RwLock::new(HashMap::new()));
		let in_packet_observer = Arc::new(RwLock::new(HashMap::new()));
		let out_packet_observer = Arc::new(RwLock::new(HashMap::new()));
		let in_command_observer = Arc::new(RwLock::new(HashMap::new()));

		let data = Self {
			is_client,
			local_addr,
			private_key,
			logger,
			udp_packet_sink,
			exit_send,
			resend_config: Default::default(),
			connections,

			in_udp_packet_observer,
			out_udp_packet_observer,
			in_packet_observer,
			out_packet_observer,
			in_command_observer,

			packet_handler,
			connection_manager,
			connection_listeners: Vec::new(),
		};

		let out_udp_packet_observer = data.out_udp_packet_observer.clone();
		tokio::spawn(
			udp_packet_sink_sender
				.map(move |(addr, p)| {
					for o in out_udp_packet_observer.read().values() {
						o.observe(addr, &p);
					}
					(p, addr)
				}).forward(sink.sink_map_err(
					move |e| error!(logger2, "Failed to send udp packet"; "error" => ?e),
				)).map(|_| ()),
		);

		// Handle incoming packets
		let logger = data.logger.clone();
		let logger2 = logger.clone();
		let in_udp_packet_observer = data.in_udp_packet_observer.clone();
		let mut codec = PacketCodecReceiver::new(&data, unknown_udp_packet_sink);
		tokio::spawn(
			stream
				.map_err(move |e| {
					error!(logger2, "Packet stream errored";
						"error" => ?e)
				}).for_each(move |(p, a)| -> Box<Future<Item=_, Error=_> + Send> {
					match InPacket::try_new(p.freeze(),
						if is_client { Direction::S2C } else { Direction::C2S }) {
						Ok(packet) => {
							for o in in_udp_packet_observer.read().values() {
								o.observe(a, &packet);
							}

							let logger = logger.clone();
							Box::new(codec.handle_udp_packet((a, packet)).then(move |r| {
								if let Err(e) = r {
									warn!(logger, "Packet handler errored"; "error" => ?e);
								}
								// Ignore errors for one packet
								Ok(())
							}))
						}
						Err(e) => {
							warn!(logger, "Packet parsing error"; "error" => ?e);
							Box::new(future::ok(()))
						}
					}
				}).select2(exit_recv)
				.map_err(|_| ())
				.map(|_| ()),
		);
		Ok(Arc::new(Mutex::new(data)))
	}

	/// Add a new connection to this socket.
	pub fn add_connection(
		&mut self,
		data_mut: Weak<Mutex<Self>>,
		mut data: CM::AssociatedData,
		addr: SocketAddr,
	) -> CM::Key {
		// Add options like ip to logger
		let logger = self.logger.new(o!("addr" => addr.to_string()));
		let resender =
			DefaultResender::new(self.resend_config.clone(), logger.clone());
		// Use an unbounded channel so try_send never fails
		let (s2c_init_send, s2c_init_recv) = mpsc::unbounded();
		let (c2s_init_send, c2s_init_recv) = mpsc::unbounded();
		let (command_send, command_recv) = mpsc::unbounded();
		let (audio_send, audio_recv) = mpsc::unbounded();
		let mut con = Connection::new(
			addr,
			resender,
			logger.clone(),
			self.udp_packet_sink.clone(),
			self.is_client,
			s2c_init_send,
			c2s_init_send,
			command_send,
			audio_send,
		);

		let mut key = self
			.connection_manager
			.new_connection_key(&mut data, &mut con);
		// Call listeners
		let mut i = 0;
		while i < self.connection_listeners.len() {
			if self.connection_listeners[i]
				.on_connection_created(&mut key, &mut data, &mut con)
			{
				self.connection_listeners.remove(i);
			} else {
				i += 1;
			}
		}

		let con_val = ConnectionValue::new(
			data,
			con,
			self.out_packet_observer.clone(),
		);
		self.packet_handler.new_connection(
			&con_val,
			s2c_init_recv.map_err(|_| format_err!("Failed to receive").into()),
			c2s_init_recv.map_err(|_| format_err!("Failed to receive").into()),
			command_recv.map_err(|_| format_err!("Failed to receive").into()),
			audio_recv.map_err(|_| format_err!("Failed to receive").into()),
		);

		// Add connection
		self.connections.write().insert(key.clone(), con_val);

		// Start resender
		let resend_fut =
			crate::resend::ResendFuture::new(&self, data_mut, key.clone());
		::tokio::spawn(resend_fut.map_err(move |e| {
			error!(logger, "Resend future failed"; "error" => ?e);
		}));

		key
	}

	pub fn remove_connection(
		&mut self,
		key: &CM::Key,
	) -> Option<ConnectionValue<CM::AssociatedData>> {
		// Get connection
		let mut res = self.get_connection(key);
		if let Some(con) = &mut res {
			// Remove connection
			self.connections.write().remove(key);

			// Call listeners
			let mut i = 0;
			while i < self.connection_listeners.len() {
				if self.connection_listeners[i].on_connection_removed(&key, con)
				{
					self.connection_listeners.remove(i);
				} else {
					i += 1;
				}
			}
		}
		res
	}

	pub fn get_connection(
		&self,
		key: &CM::Key,
	) -> Option<ConnectionValue<CM::AssociatedData>> {
		self.connections.read().get(key).map(|v| v.clone())
	}

	pub fn add_in_udp_packet_observer(&mut self, key: String, o: Box<InUdpPacketObserver>) {
		self.in_udp_packet_observer.write().insert(key, o);
	}
	pub fn add_out_udp_packet_observer(&mut self, key: String, o: Box<OutUdpPacketObserver>) {
		self.out_udp_packet_observer.write().insert(key, o);
	}
	pub fn remove_in_udp_packet_observer(&mut self, key: &String) {
		self.in_udp_packet_observer.write().remove(key);
	}
	pub fn remove_out_udp_packet_observer(&mut self, key: &String) {
		self.out_udp_packet_observer.write().remove(key);
	}

	pub fn add_in_packet_observer(&mut self, key: String, o: Box<InPacketObserver<CM::AssociatedData>>) {
		self.in_packet_observer.write().insert(key, o);
	}
	pub fn add_out_packet_observer(&mut self, key: String, o: Box<OutPacketObserver<CM::AssociatedData>>) {
		self.out_packet_observer.write().insert(key, o);
	}
	pub fn remove_in_packet_observer(&mut self, key: &String) {
		self.in_packet_observer.write().remove(key);
	}
	pub fn remove_out_packet_observer(&mut self, key: &String) {
		self.out_packet_observer.write().remove(key);
	}

	pub fn add_in_command_observer(&mut self, key: String, o: Box<InCommandObserver<CM::AssociatedData>>) {
		self.in_command_observer.write().insert(key, o);
	}
	pub fn remove_in_command_observer(&mut self, key: &String) {
		self.in_command_observer.write().remove(key);
	}
}
