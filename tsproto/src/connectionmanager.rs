use std::hash::Hash;
use std::marker::PhantomData;
use std::net::SocketAddr;

use bytes::Bytes;
use futures::Sink;

use crate::connection::Connection;
use crate::handler_data::PacketHandler;
use crate::packets::{InPacket, PacketType};
use crate::Error;

/// The unique identification of a connection is handled by the implementation.
pub trait ConnectionManager: Send {
	/// The key wihch identifies a connection.
	type Key: Clone + Eq + Hash + Send + Sync;

	/// Data which is associated with each connection. This can be used to store
	/// additional connection information.
	type AssociatedData: Send + 'static;

	type PacketHandler: PacketHandler<Self::AssociatedData>;

	fn new_connection_key(
		&mut self,
		data: &mut Self::AssociatedData,
		con: &mut Connection,
	) -> Self::Key;

	/// Compute the connection key for an incoming udp packet.
	fn get_connection_key(
		src_addr: SocketAddr,
		udp_packet: &InPacket,
	) -> Self::Key;
}

/// Events to inform a resender of the current state of a connection.
#[derive(PartialEq, Eq, Debug, Hash)]
pub enum ResenderEvent {
	/// The connection is starting
	Connecting,
	/// The handshake is completed, this is the normal operation mode
	Connected,
	/// The connection is tearing down
	Disconnecting,
}

/// For each connection, a resender is created, which is responsible for sending
/// command packets and ensure, that they are delivered.
///
/// This is accomplished by implementing a sink, which takes the packet type, id
/// and the packet itself. The id must be [`Command`] or [`CommandLow`].
///
/// You should note that the resending should be implemented independant of the
/// sink, so it is possible to put two packets into the sink while no ack has
/// been received.
///
/// [`Command`]: ../commands/struct.Command.html
/// [`CommandLow`]: ../commands/struct.CommandLow.html
pub trait Resender:
	Sink<SinkItem = (PacketType, u16, Bytes), SinkError = Error>
{
	/// Called for a received ack packet.
	///
	/// The packet type must be [`Command`] or [`CommandLow`].
	///
	/// [`Command`]: ../commands/struct.Command.html
	/// [`CommandLow`]: ../commands/struct.CommandLow.html
	fn ack_packet(&mut self, p_type: PacketType, p_id: u16);

	/// The resender can block outgoing voice packets.
	///
	/// Return `true` to allow sending and `false` to block packets.
	fn send_voice_packets(&self, p_type: PacketType) -> bool;

	/// If there are packets in the queue which were not acknowledged.
	fn is_empty(&self) -> bool;

	/// This method informs the resender of state changes of the connection.
	fn handle_event(&mut self, event: ResenderEvent);

	/// Called for received udp packets.
	fn udp_packet_received(&mut self, packet: &Bytes);
}

/// An implementation of a connectionmanager, that identifies a connection its
/// socket.
///
/// `T` contains associated data that will be saved for each connection.
pub struct SocketConnectionManager<PH: PacketHandler<T>, T: Send + 'static> {
	phantom: PhantomData<T>,
	phantom2: PhantomData<PH>,
}

impl<PH: PacketHandler<T>, T: Send + 'static> Default
	for SocketConnectionManager<PH, T>
{
	fn default() -> Self {
		SocketConnectionManager {
			phantom: PhantomData,
			phantom2: PhantomData,
		}
	}
}

impl<PH: PacketHandler<T>, T: Send + 'static> SocketConnectionManager<PH, T> {
	/// Create a new connection manager.
	pub fn new() -> Self {
		Self::default()
	}
}

impl<PH: PacketHandler<T>, T: Send + 'static> ConnectionManager
	for SocketConnectionManager<PH, T>
{
	type Key = SocketAddr;
	type AssociatedData = T;
	type PacketHandler = PH;

	fn new_connection_key(
		&mut self,
		_: &mut Self::AssociatedData,
		con: &mut Connection,
	) -> Self::Key {
		con.address
	}

	fn get_connection_key(
		addr: SocketAddr,
		_: &InPacket,
	) -> Self::Key {
		addr
	}
}
