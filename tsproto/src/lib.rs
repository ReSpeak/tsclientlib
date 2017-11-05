// Limit for error_chain
#![recursion_limit = "1024"]
#![cfg_attr(feature = "cargo-clippy",
           allow(redundant_closure_call, clone_on_ref_ptr))]

extern crate base64;
extern crate byteorder;
extern crate chrono;
#[macro_use]
extern crate error_chain;
extern crate futures;
#[macro_use]
extern crate nom;
extern crate num;
#[macro_use]
extern crate num_derive;
extern crate quicklz;
extern crate rand;
extern crate ring;
#[macro_use]
extern crate slog;
extern crate slog_async;
extern crate slog_perf;
extern crate slog_term;
extern crate tokio_core;
extern crate tomcrypt;

use std::{fmt, io, str};
use std::net::SocketAddr;

use futures::Future;
use tokio_core::net::UdpCodec;

use packets::UdpPacket;

#[allow(unused_doc_comment)]
pub mod errors {
    // Create the Error, ErrorKind, ResultExt, and Result types
    error_chain! {
        foreign_links {
            Io(::std::io::Error);
            Ring(::ring::error::Unspecified);
            Base64(::base64::DecodeError);
            Utf8(::std::str::Utf8Error);
            ParseInt(::std::num::ParseIntError);
            FutureCanceled(::futures::Canceled);
        }
        links {
            Tomcrypt(::tomcrypt::errors::Error, ::tomcrypt::errors::ErrorKind);
            Quicklz(::quicklz::errors::Error, ::quicklz::errors::ErrorKind);
        }
    }
}
use errors::*;

macro_rules! tryf {
    ($e:expr) => {
        match $e {
            Ok(e) => e,
            Err(error) => return Box::new(future::err(error.into())),
        }
    };
}

pub mod algorithms;
pub mod client;
pub mod commands;
pub mod handler_data;
pub mod log;
pub mod packets;
pub mod packet_codec;

type BoxFuture<T, E> = Box<Future<Item = T, Error = E>>;
type Map<K, V> = std::collections::HashMap<K, V>;

/// The maximum number of bytes for a fragmented packet.
#[cfg_attr(feature = "cargo-clippy", allow(unreadable_literal))]
const MAX_FRAGMENTS_LENGTH: usize = 40960;
/// The maximum number of packets that are stored, if they receive out-of-order.
const MAX_QUEUE_LEN: usize = 50;
/// The maximum decompressed size of a packet.
#[cfg_attr(feature = "cargo-clippy", allow(unreadable_literal))]
const MAX_DECOMPRESSED_SIZE: u32 = 40960;
const FAKE_KEY: &str = "c:\\windows\\syste";
const FAKE_NONCE: &str = "m\\firewall32.cpl";
/// How long packets are resent until the connection is closed.
const TIMEOUT_SECONDS: i64 = 30;

#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub struct ClientId(pub SocketAddr);
#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub struct ServerId(pub SocketAddr);

#[derive(Default)]
struct TsCodec;

struct HexSlice<'a, T: fmt::LowerHex + 'a>(&'a [T]);

impl<'a> fmt::Debug for HexSlice<'a, u8> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "[")?;
        if let Some((l, m)) = self.0.split_last() {
            for b in m {
                if *b == 0 {
                    write!(f, "0, ")?;
                } else {
                    write!(f, "{:#02x}, ", b)?;
                }
            }
            if *l == 0 {
                write!(f, "0")?;
            } else {
                write!(f, "{:#02x}", l)?;
            }
        }
        write!(f, "]")
    }
}

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
        let mut res = Vec::with_capacity(buf.len());
        res.extend_from_slice(buf);
        Ok((*src, UdpPacket(res)))
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

pub fn init() -> Result<()> {
    tomcrypt::init();
    tomcrypt::register_sprng()?;
    tomcrypt::register_rijndael_cipher()?;
    Ok(())
}
