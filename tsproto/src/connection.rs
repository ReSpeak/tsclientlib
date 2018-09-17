use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fmt;
use std::net::SocketAddr;
use std::rc::Rc;
use std::u16;

use bytes::Bytes;
use futures::{future, Future, Sink};
use futures::sync::mpsc;
use num::ToPrimitive;
use slog;

use Error;
use crypto::EccKeyPubP256;
use packets::*;
use resend::DefaultResender;

/// A cache for the key and nonce for a generation id.
/// This has to be stored for each packet type.
#[derive(Debug)]
pub struct CachedKey {
    /// The generation id
    pub generation_id: u32,
    /// The key
    pub key: [u8; 16],
    /// The nonce
    pub nonce: [u8; 16],
}

impl Default for CachedKey {
    fn default() -> Self {
        CachedKey {
            generation_id: u32::max_value(),
            key: [0; 16],
            nonce: [0; 16],
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
            SharedIv::ProtocolOrig(ref data) =>
                write!(f, "SharedIv::ProtocolOrig({:?})",
                    ::utils::HexSlice(data)),
            SharedIv::Protocol31(ref data) =>
                write!(f, "SharedIv::Protocol32({:?})",
                    ::utils::HexSlice(data)),
        }
    }
}

/// Data that has to be stored for a connection when it is connected.
#[derive(Debug)]
pub struct ConnectedParams {
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
    pub receive_queue: [Vec<(Header, Vec<u8>)>; 2],
    /// Used for incoming fragmented packets.
    ///
    /// Only used for `Command` and `CommandLow` packets.
    pub fragmented_queue: [Option<(Header, Vec<u8>)>; 2],
    /// Used for incoming fragmented packets.
    ///
    /// Contains the audio initialization buffer for each client. These packets
    /// are marked with the `compressed` flag. Usually 3 of them are sent at the
    /// beginning of each transmission.
    /// Only used for `Voice` and `VoiceWhisper` packets.
    pub voice_fragmented_queue: [BTreeMap<u16, Vec<u8>>; 2],
    /// The next packet id that is expected.
    ///
    /// Works like the `outgoing_p_ids`.
    pub incoming_p_ids: [(u32, u16); 8],

    /// The client id of this connection.
    pub c_id: u16,
    /// If voice packets should be encrypted
    pub voice_encryption: bool,

    pub public_key: EccKeyPubP256,
    /// The iv used to encrypt and decrypt packets.
    pub shared_iv: SharedIv,
    /// The mac used for unencrypted packets.
    pub shared_mac: [u8; 8],
    /// Cached key and nonce per packet type.
    pub key_cache: [CachedKey; 8],
}

impl ConnectedParams {
    /// Fills the parameters for a connection with their default state.
    pub fn new(public_key: EccKeyPubP256, shared_iv: SharedIv,
        shared_mac: [u8; 8]) -> Self {
        Self {
            outgoing_p_ids: Default::default(),
            receive_queue: Default::default(),
            fragmented_queue: Default::default(),
            voice_fragmented_queue: Default::default(),
            incoming_p_ids: Default::default(),
            c_id: 0,
            voice_encryption: true,
            public_key,
            shared_iv,
            shared_mac,
            key_cache: Default::default(),
        }
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
    ) -> (bool, u32, u16, u16) {
        let type_i = p_type.to_usize().unwrap();
        // Receive window is the next half of ids
        let cur_next = self.incoming_p_ids[type_i].1;
        let limit = ((u32::from(cur_next) + u32::from(u16::MAX) / 2)
            % u32::from(u16::MAX)) as u16;
        let gen = self.incoming_p_ids[type_i].0;
        (
            (cur_next < limit && p_id >= cur_next && p_id < limit)
                || (cur_next > limit && (p_id >= cur_next || p_id < limit)),
            if p_id >= cur_next { gen } else { gen + 1 },
            cur_next,
            limit,
        )
    }
}

/// Represents a currently alive connection.
pub struct Connection {
    /// A logger for this connection.
    pub logger: slog::Logger,
    /// The parameters of this connection, if it is already established.
    pub params: Option<ConnectedParams>,
    /// The adress of the other side, where packets are coming from and going
    /// to.
    pub address: SocketAddr,

    pub resender: DefaultResender,
    udp_packet_sink: mpsc::Sender<(SocketAddr, Bytes)>,
}

impl Connection {
    /// Creates a new connection struct.
    pub fn new(address: SocketAddr, resender: DefaultResender,
        logger: slog::Logger, udp_packet_sink: mpsc::Sender<(SocketAddr, Bytes)>)
        -> Rc<RefCell<Self>> {
        Rc::new(RefCell::new(Self {
            logger,
            params: None,
            address,
            resender,
            udp_packet_sink,
        }))
    }

    pub fn send_udp_packet(&mut self, p_type: PacketType, p_id: u16, packet: Bytes) -> impl Future<Item=(), Error=Error> {
        // Use the resender for important packets
        match p_type {
            PacketType::Init | PacketType::Command | PacketType::CommandLow => {
            }
            _ => {
            }
        }
        self.udp_packet_sink.clone()
            .send((self.address, packet))
            .map(|_| ())
            .map_err(|e| format_err!("Failed to send udp packet ({:?})", e).into())
    }

    pub fn send_packet(&mut self, packet: Packet) -> impl Future<Item=(), Error=Error> {
        future::ok(panic!())
    }
}
