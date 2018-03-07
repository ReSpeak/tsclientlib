use std::cell::RefCell;
use std::net::SocketAddr;
use std::rc::Rc;
use std::u16;

use slog;
use num::ToPrimitive;

use connectionmanager::ConnectionManager;
use packets::*;

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
    /// The next packet id that is expected.
    ///
    /// Works like the `outgoing_p_ids`.
    pub incoming_p_ids: [(u32, u16); 8],

    /// The client id of this connection.
    pub c_id: u16,
    /// If voice packets should be encrypted
    pub voice_encryption: bool,

    pub public_key: ::crypto::EccKey,
    /// The iv used to encrypt and decrypt packets.
    pub shared_iv: [u8; 20],
    /// The mac used for unencrypted packets.
    pub shared_mac: [u8; 8],
    /// Cached key and nonce per packet type.
    pub key_cache: [CachedKey; 8],
}

impl ConnectedParams {
    /// Fills the parameters for a connection with their default state.
    pub fn new(public_key: ::crypto::EccKey, shared_iv: [u8; 20], shared_mac: [u8; 8]) -> Self {
        Self {
            outgoing_p_ids: Default::default(),
            receive_queue: Default::default(),
            fragmented_queue: Default::default(),
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
    pub(crate) fn in_receive_window(
        &self,
        p_type: PacketType,
        p_id: u16,
    ) -> (bool, u16, u16) {
        let type_i = p_type.to_usize().unwrap();
        // Receive window is the next half of ids
        let cur_next = self.incoming_p_ids[type_i].1;
        let limit = ((u32::from(cur_next) + u32::from(u16::MAX) / 2)
            % u32::from(u16::MAX)) as u16;
        (
            (cur_next < limit && p_id >= cur_next && p_id < limit)
                || (cur_next > limit && (p_id >= cur_next || p_id < limit)),
            cur_next,
            limit,
        )
    }
}

/// Represents a currently alive connection.
pub struct Connection<CM: ConnectionManager + 'static> {
    /// A logger for this connection.
    pub logger: slog::Logger,
    /// The parameters of this connection, if it is already established.
    pub params: Option<ConnectedParams>,
    /// The adress of the other side, where packets are coming from and going
    /// to.
    pub address: SocketAddr,

    pub resender: CM::Resend,
}

impl<CM: ConnectionManager + 'static> Connection<CM> {
    /// Creates a new connection struct.
    pub fn new(address: SocketAddr, resender: CM::Resend, logger: slog::Logger)
        -> Rc<RefCell<Self>> {
        Rc::new(RefCell::new(Self {
            logger,
            params: None,
            address,
            resender,
        }))
    }
}
