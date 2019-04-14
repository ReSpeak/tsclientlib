use std::fmt;
use std::net::SocketAddr;
use std::u16;

use aes::block_cipher_trait::generic_array::typenum::consts::U16;
use aes::block_cipher_trait::generic_array::GenericArray;
use bytes::Bytes;
use failure::format_err;
use futures::sync::mpsc;
use futures::{self, AsyncSink, Sink};
use num_traits::ToPrimitive;
use slog;

use crate::crypto::EccKeyPubP256;
use crate::handler_data::{ConnectionValue, ConnectionValueWeak};
use crate::packets::*;
use crate::resend::DefaultResender;
use crate::Error;

/// A cache for the key and nonce for a generation id.
/// This has to be stored for each packet type.
#[derive(Debug)]
pub struct CachedKey {
	/// The generation id
	pub generation_id: u32,
	/// The key
	pub key: GenericArray<u8, U16>,
	/// The nonce
	pub nonce: GenericArray<u8, U16>,
}

impl Default for CachedKey {
	fn default() -> Self {
		CachedKey {
			generation_id: u32::max_value(),
			key: [0; 16].into(),
			nonce: [0; 16].into(),
		}
	}
}

pub enum SharedIv {
	/// The protocol until TeamSpeak server 3.1 (excluded) uses this format.
	ProtocolOrig([u8; 20]),
	/// The protocol since TeamSpeak server 3.1 uses this format.
	Protocol31([u8; 64]),
}

impl fmt::Debug for SharedIv {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		match *self {
			SharedIv::ProtocolOrig(ref data) => write!(
				f,
				"SharedIv::ProtocolOrig({:?})",
				crate::utils::HexSlice(data)
			),
			SharedIv::Protocol31(ref data) => write!(
				f,
				"SharedIv::Protocol32({:?})",
				crate::utils::HexSlice(data)
			),
		}
	}
}

/// Data that has to be stored for a connection when it is connected.
#[derive(Debug)]
pub struct ConnectedParams {
	/// The client id of this connection.
	pub c_id: u16,
	/// If voice packets should be encrypted
	pub voice_encryption: bool,

	/// The public key of the other side.
	pub public_key: EccKeyPubP256,
	/// The iv used to encrypt and decrypt packets.
	pub shared_iv: SharedIv,
	/// The mac used for unencrypted packets.
	pub shared_mac: [u8; 8],
	/// Cached key and nonce per packet type and for server to client (without
	/// client id inside the packet) and client to server communication.
	pub key_cache: [[CachedKey; 2]; 8],
}

impl ConnectedParams {
	/// Fills the parameters for a connection with their default state.
	pub fn new(
		public_key: EccKeyPubP256,
		shared_iv: SharedIv,
		shared_mac: [u8; 8],
	) -> Self
	{
		Self {
			c_id: 0,
			voice_encryption: true,
			public_key,
			shared_iv,
			shared_mac,
			key_cache: Default::default(),
		}
	}
}

/// Represents a currently alive connection.
#[derive(Debug)]
pub struct Connection {
	pub is_client: bool,
	/// A logger for this connection.
	pub logger: slog::Logger,
	/// The parameters of this connection, if it is already established.
	pub params: Option<ConnectedParams>,
	/// The adress of the other side, where packets are coming from and going
	/// to.
	pub address: SocketAddr,

	pub resender: DefaultResender,
	udp_packet_sink: mpsc::Sender<(SocketAddr, Bytes)>,
	pub s2c_init_sink: mpsc::UnboundedSender<InS2CInit>,
	pub c2s_init_sink: mpsc::UnboundedSender<InC2SInit>,
	pub command_sink: mpsc::UnboundedSender<InCommand>,
	pub audio_sink: mpsc::UnboundedSender<InAudio>,

	/// The next packet id that should be sent.
	///
	/// This list is indexed by the [`PacketType`], [`PacketType::Init`] is an
	/// invalid index.
	///
	/// [`PacketType`]: udp/enum.PacketType.html
	/// [`PacketType::Init`]: udp/enum.PacketType.html
	pub outgoing_p_ids: [(u32, u16); 8],
	/// Used for incoming out-of-order packets.
	///
	/// Only used for `Command` and `CommandLow` packets.
	pub receive_queue: [Vec<InPacket>; 2],
	/// Used for incoming fragmented packets.
	///
	/// Only used for `Command` and `CommandLow` packets.
	pub fragmented_queue: [Option<(InPacket, Vec<u8>)>; 2],
	/// The next packet id that is expected.
	///
	/// Works like the `outgoing_p_ids`.
	pub incoming_p_ids: [(u32, u16); 8],
}

impl Connection {
	/// Creates a new connection struct.
	pub fn new(
		address: SocketAddr,
		resender: DefaultResender,
		logger: slog::Logger,
		udp_packet_sink: mpsc::Sender<(SocketAddr, Bytes)>,
		is_client: bool,
		s2c_init_sink: mpsc::UnboundedSender<InS2CInit>,
		c2s_init_sink: mpsc::UnboundedSender<InC2SInit>,
		command_sink: mpsc::UnboundedSender<InCommand>,
		audio_sink: mpsc::UnboundedSender<InAudio>,
	) -> Self
	{
		let mut res = Self {
			is_client,
			logger,
			params: None,
			address,
			resender,
			udp_packet_sink,
			s2c_init_sink,
			c2s_init_sink,
			command_sink,
			audio_sink,

			outgoing_p_ids: Default::default(),
			receive_queue: Default::default(),
			fragmented_queue: Default::default(),
			incoming_p_ids: Default::default(),
		};
		if is_client {
			// The first command is sent as part of the C2SInit::Init4 packet
			// so it does not get registered automatically.
			res.outgoing_p_ids[PacketType::Command.to_usize().unwrap()] =
				(0, 1);
		} else {
			res.incoming_p_ids[PacketType::Command.to_usize().unwrap()] =
				(0, 1);
		}
		res
	}

	/// Check if a given id is in the receive window.
	///
	/// Returns
	/// 1. If the packet id is inside the receive window
	/// 1. The generation of the packet
	/// 1. The minimum accepted packet id
	/// 1. The maximum accepted packet id
	pub(crate) fn in_receive_window(
		&self,
		p_type: PacketType,
		p_id: u16,
	) -> (bool, u32, u16, u16)
	{
		if p_type == PacketType::Init {
			return (true, 0, 0, 0);
		}
		let type_i = p_type.to_usize().unwrap();
		// Receive window is the next half of ids
		let cur_next = self.incoming_p_ids[type_i].1;
		let (limit, next_gen) = cur_next.overflowing_add(u16::MAX / 2);
		let gen = self.incoming_p_ids[type_i].0;
		(
			(!next_gen && p_id >= cur_next && p_id < limit)
				|| (next_gen && (p_id >= cur_next || p_id < limit)),
			if next_gen && p_id < limit {
				gen + 1
			} else {
				gen
			},
			cur_next,
			limit,
		)
	}
}

pub struct ConnectionUdpPacketSink<T: Send + 'static> {
	con: ConnectionValueWeak<T>,
	address: SocketAddr,
	udp_packet_sink: mpsc::Sender<(SocketAddr, Bytes)>,
}

impl<T: Send + 'static> ConnectionUdpPacketSink<T> {
	pub fn new(con: &ConnectionValue<T>) -> Self {
		let address;
		let udp_packet_sink;
		{
			let con = con.mutex.lock();
			address = con.1.address;
			udp_packet_sink = con.1.udp_packet_sink.clone();
		}

		Self {
			con: con.downgrade(),
			address,
			udp_packet_sink,
		}
	}
}

impl<T: Send + 'static> Sink for ConnectionUdpPacketSink<T> {
	type SinkItem = (PacketType, u32, u16, Bytes);
	type SinkError = Error;

	fn start_send(
		&mut self,
		(p_type, p_gen, p_id, udp_packet): Self::SinkItem,
	) -> futures::StartSend<Self::SinkItem, Self::SinkError>
	{
		match p_type {
			PacketType::Init | PacketType::Command | PacketType::CommandLow => {
				if let Some(mutex) = self.con.mutex.upgrade() {
					let mut con = mutex.lock();
					con.1.resender.start_send((p_type, p_gen, p_id, udp_packet))
				} else {
					Err(format_err!("Connection is gone").into())
				}
			}
			_ => Ok(
				match self
					.udp_packet_sink
					.start_send((self.address, udp_packet))
					.map_err(|e| {
						format_err!("Failed to send udp packet ({:?})", e)
					})? {
					AsyncSink::Ready => AsyncSink::Ready,
					AsyncSink::NotReady((_, p)) => {
						AsyncSink::NotReady((p_type, p_gen, p_id, p))
					}
				},
			),
		}
	}

	fn poll_complete(&mut self) -> futures::Poll<(), Self::SinkError> {
		self.udp_packet_sink.poll_complete().map_err(|e| {
			format_err!("Failed to complete sending udp packet ({:?})", e)
				.into()
		})
	}
}
