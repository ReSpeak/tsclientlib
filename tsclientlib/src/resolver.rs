//! Resolve TeamSpeak server addresses of any kind.
//!
//! Using
//! 1. Server nicknames
//! 1. TSDNS (SRV records)
//! 1. Normal DNS (A and AAAA records)
//! 1. The address directly if it is an ip
//!
//! A port is determined automatically, if it is not specified.

use std::net::SocketAddr;

use Result;

pub fn resolve() -> Result<SocketAddr> {
}
