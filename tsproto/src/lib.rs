#![cfg_attr(feature = "cargo-clippy",
    allow(redundant_closure_call, clone_on_ref_ptr, let_and_return,
    useless_format))]

extern crate base64;
extern crate byteorder;
extern crate chrono;
extern crate curve25519_dalek;
#[macro_use]
extern crate failure;
extern crate futures;
#[cfg(feature = "rust-gmp")]
extern crate gmp;
#[macro_use]
extern crate nom;
extern crate num;
#[macro_use]
extern crate num_derive;
extern crate openssl;
extern crate quicklz;
extern crate rand;
extern crate ring;
#[macro_use]
extern crate slog;
extern crate slog_async;
extern crate slog_perf;
extern crate slog_term;
extern crate tokio_core;
extern crate yasna;

use std::io;
use std::net::SocketAddr;

use failure::ResultExt;
use futures::{Future, Sink, Stream};
use tokio_core::net::UdpCodec;

use packets::UdpPacket;

pub mod algorithms;
pub mod client;
pub mod commands;
pub mod connection;
pub mod connectionmanager;
pub mod crypto;
pub mod handler_data;
pub mod license;
pub mod log;
pub mod packets;
pub mod packet_codec;
pub mod resend;
pub mod utils;

type BoxFuture<T, E> = Box<Future<Item = T, Error = E>>;
type Map<K, V> = std::collections::HashMap<K, V>;
type Result<T> = std::result::Result<T, Error>;

/// The maximum number of bytes for a fragmented packet.
#[cfg_attr(feature = "cargo-clippy", allow(unreadable_literal))]
const MAX_FRAGMENTS_LENGTH: usize = 40960;
/// The maximum number of packets which are stored, if they are received
/// out-of-order.
const MAX_QUEUE_LEN: usize = 50;
/// The maximum decompressed size of a packet.
#[cfg_attr(feature = "cargo-clippy", allow(unreadable_literal))]
const MAX_DECOMPRESSED_SIZE: u32 = 40960;
const FAKE_KEY: [u8; 16] = *b"c:\\windows\\syste";
const FAKE_NONCE: [u8; 16] = *b"m\\firewall32.cpl";

#[derive(Fail, Debug)]
pub enum Error {
    #[fail(display = "{}", _0)]
    Io(#[cause] std::io::Error),
    #[fail(display = "{}", _0)]
    Ring(#[cause] ring::error::Unspecified),
    #[fail(display = "{}", _0)]
    Base64(#[cause] base64::DecodeError),
    #[fail(display = "{}", _0)]
    Utf8(#[cause] std::str::Utf8Error),
    #[fail(display = "{}", _0)]
    ParseInt(#[cause] std::num::ParseIntError),
    #[fail(display = "{}", _0)]
    FutureCanceled(#[cause] futures::Canceled),
    #[fail(display = "{}", _0)]
    Openssl(#[cause] openssl::error::ErrorStack),
    #[fail(display = "{}", _0)]
    Yasna(#[cause] yasna::ASN1Error),
    #[fail(display = "{}", _0)]
    Quicklz(#[cause] quicklz::Error),
    #[fail(display = "{}", _0)]
    ParsePacket(String),
    #[fail(display = "Packet {} not in receive window [{};{}) for type {:?}",
        id, next, limit, p_type)]
    NotInReceiveWindow {
        id: u16,
        next: u16,
        limit: u16,
        p_type: packets::PacketType,
    },
    #[fail(display = "Got unallowed unencrypted packet")]
    UnallowedUnencryptedPacket,
    #[fail(display = "Got unexpected init packet")]
    UnexpectedInitPacket,
    #[fail(display = "Maximum length exceeded for {}", _0)]
    MaxLengthExceeded(String),
    #[fail(display = "Cannot parse command ({})", _0)]
    ParseCommand(String),
    #[fail(display = "Wrong signature")]
    WrongSignature,
    #[fail(display = "{}", _0)]
    Other(#[cause] failure::Compat<failure::Error>),
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

impl From<ring::error::Unspecified> for Error {
    fn from(e: ring::error::Unspecified) -> Self {
        Error::Ring(e)
    }
}

impl From<base64::DecodeError> for Error {
    fn from(e: base64::DecodeError) -> Self {
        Error::Base64(e)
    }
}

impl From<std::str::Utf8Error> for Error {
    fn from(e: std::str::Utf8Error) -> Self {
        Error::Utf8(e)
    }
}

impl From<std::num::ParseIntError> for Error {
    fn from(e: std::num::ParseIntError) -> Self {
        Error::ParseInt(e)
    }
}

impl From<futures::Canceled> for Error {
    fn from(e: futures::Canceled) -> Self {
        Error::FutureCanceled(e)
    }
}

impl From<openssl::error::ErrorStack> for Error {
    fn from(e: openssl::error::ErrorStack) -> Self {
        Error::Openssl(e)
    }
}

impl From<yasna::ASN1Error> for Error {
    fn from(e: yasna::ASN1Error) -> Self {
        Error::Yasna(e)
    }
}

impl From<quicklz::Error> for Error {
    fn from(e: quicklz::Error) -> Self {
        Error::Quicklz(e)
    }
}

impl From<failure::Error> for Error {
    fn from(e: failure::Error) -> Self {
        let r: std::result::Result<(), _> = Err(e);
        Error::Other(r.compat().unwrap_err())
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub struct ClientId(pub SocketAddr);
#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub struct ServerId(pub SocketAddr);

#[derive(Default)]
struct TsCodec;

impl Into<SocketAddr> for ClientId {
    fn into(self) -> SocketAddr {
        self.0
    }
}
impl Into<SocketAddr> for ServerId {
    fn into(self) -> SocketAddr {
        self.0
    }
}

impl UdpCodec for TsCodec {
    type In = (SocketAddr, UdpPacket);
    type Out = (SocketAddr, UdpPacket);

    fn decode(&mut self, src: &SocketAddr, buf: &[u8]) -> io::Result<Self::In> {
        Ok((*src, UdpPacket(buf.to_vec())))
    }

    /// The input packet has to be compressed, encrypted and fragmented already.
    fn encode(
        &mut self,
        (addr, UdpPacket(mut packet)): Self::Out,
        buf: &mut Vec<u8>,
    ) -> SocketAddr {
        buf.append(&mut packet);
        addr
    }
}

/// A trait for a `Stream` wrapper.
///
/// Implementors of this trait supply just one method `wrap`, that takes a
/// stream as an argument and returns a wrapped stream of the same type.
pub trait StreamWrapper<I, E, T: Stream<Item = I, Error = E>> {
    /// The type of additional arguments for the `wrap` function.
    type A;
    /// The resulting stream type.
    type Result: Stream<Item = I, Error = E>;

    /// `A` holds additional arguments.
    fn wrap(inner: T, a: Self::A) -> Self::Result;
}

/// A trait for a `Sink` wrapper.
///
/// Implementors of this trait supply just one method `wrap`, that takes a
/// sink as an argument and returns a wrapped sink of the same type.
pub trait SinkWrapper<I, E, T: Sink<SinkItem = I, SinkError = E>> {
    /// The type of additional arguments for the `wrap` function.
    type A;
    /// The resulting sink type.
    type Result: Sink<SinkItem = I, SinkError = E>;

    /// `A` holds additional arguments.
    fn wrap(inner: T, a: Self::A) -> Self::Result;
}

pub fn init() -> Result<()> {
    openssl::init();
    Ok(())
}
