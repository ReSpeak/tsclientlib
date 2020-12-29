use std::net::IpAddr;

use crate::{Error, Result};

/// Try to approximate the not stabilized ip.is_global().
pub fn is_global_ip(ip: &IpAddr) -> bool {
	if !ip.is_unspecified() && !ip.is_loopback() && !ip.is_multicast() {
		match *ip {
			IpAddr::V4(ref ip) => !ip.is_broadcast() && !ip.is_link_local() && !ip.is_private(),
			IpAddr::V6(_) => true,
		}
	} else {
		false
	}
}

pub fn read_hex(s: &str) -> Result<Vec<u8>> {
	// Detect hex format
	if s.chars().nth(2) == Some(':') {
		// Wireshark
		Ok(s.split(':')
			.map(|s| u8::from_str_radix(s, 16))
			.collect::<::std::result::Result<Vec<_>, _>>()
			.map_err(Error::InvalidHex)?)
	} else if s.chars().nth(2) == Some(' ') {
		// Wireshark
		Ok(s.split(' ')
			.map(|s| u8::from_str_radix(s, 16))
			.collect::<::std::result::Result<Vec<_>, _>>()
			.map_err(Error::InvalidHex)?)
	} else if s.starts_with("0x") {
		let s: String = s.chars().filter(|c| !c.is_whitespace()).collect();
		// Own dumps
		Ok(s[2..]
			.split(',')
			.map(|s| u8::from_str_radix(s.trim_start_matches("0x"), 16))
			.collect::<::std::result::Result<Vec<_>, _>>()
			.map_err(Error::InvalidHex)?)
	} else {
		let s: String = s.chars().filter(|c| !c.is_whitespace()).collect();
		Ok(s.as_bytes()
			.chunks(2)
			.map(|s| u8::from_str_radix(::std::str::from_utf8(s).unwrap(), 16))
			.collect::<::std::result::Result<Vec<_>, _>>()
			.map_err(Error::InvalidHex)?)
	}
}
