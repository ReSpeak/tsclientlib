use std::hash::Hash;
use std::marker::PhantomData;
use std::net::SocketAddr;

use bytes::Bytes;
use futures::Sink;

use Error;
use connection::Connection;
use packets::PacketType;

/// The unique identification of a connection is handled by the implementation.
pub trait ConnectionManager: Send {
    /// The key wihch identifies a connection.
    type Key: Clone + Eq + Hash + Send;

    /// Data which is associated with each connection. This can be used to store
    /// additional connection information.
    type AssociatedData: Send;

    fn new_connection_key(&mut self, data: &mut Self::AssociatedData,
        con: &mut Connection) -> Self::Key;

    /// Compute the connection key for an incoming udp packet.
    fn get_connection_key(src_addr: SocketAddr,
        udp_packet: &::packets::Header) -> Self::Key;
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
/// [`Command`]:
/// [`CommandLow`]:
pub trait Resender: Sink<SinkItem = (PacketType, u16, Bytes),
    SinkError = Error> {
    /// Called for a received ack packet.
    ///
    /// The packet type must be [`Command`] or [`CommandLow`].
    ///
    /// [`Command`]:
    /// [`CommandLow`]:
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
pub struct SocketConnectionManager<T: Send> {
    phantom: PhantomData<T>,
}

impl<T: Send> Default for SocketConnectionManager<T> {
    fn default() -> Self { SocketConnectionManager { phantom: PhantomData } }
}

impl<T: Send> SocketConnectionManager<T> {
    /// Create a new connection manager.
    pub fn new() -> Self {
        Self::default()
    }
}

impl<T: Send> ConnectionManager for SocketConnectionManager<T> {
    type Key = SocketAddr;
    type AssociatedData = T;

    fn new_connection_key(&mut self, data: &mut Self::AssociatedData,
        con: &mut Connection) -> Self::Key {
        con.address
    }

    fn get_connection_key(addr: SocketAddr, _: &::packets::Header) -> Self::Key {
        addr
    }
}
