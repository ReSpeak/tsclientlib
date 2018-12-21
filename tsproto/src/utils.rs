use std::fmt;
use std::net::IpAddr;

use futures;

use crate::Result;

pub struct HexSlice<'a, T: fmt::LowerHex + 'a>(pub &'a [T]);

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

/// Try to approximate the not stabilized ip.is_global().
pub fn is_global_ip(ip: &IpAddr) -> bool {
	if !ip.is_unspecified() && !ip.is_loopback() && !ip.is_multicast() {
		match *ip {
			IpAddr::V4(ref ip) => {
				!ip.is_broadcast() && !ip.is_link_local() && !ip.is_private()
			}
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
			.collect::<::std::result::Result<Vec<_>, _>>()?)
	} else if s.chars().nth(2) == Some(' ') {
		// Wireshark
		Ok(s.split(' ')
			.map(|s| u8::from_str_radix(s, 16))
			.collect::<::std::result::Result<Vec<_>, _>>()?)
	} else if s.starts_with("0x") {
		let s: String = s.chars().filter(|c| !c.is_whitespace()).collect();
		// Own dumps
		Ok(s[2..]
			.split(',')
			.map(|s| {
				if s.starts_with("0x") {
					u8::from_str_radix(&s[2..], 16)
				} else {
					u8::from_str_radix(s, 16)
				}
			})
			.collect::<::std::result::Result<Vec<_>, _>>()?)
	} else {
		let s: String = s.chars().filter(|c| !c.is_whitespace()).collect();
		Ok(s.as_bytes()
			.chunks(2)
			.map(|s| u8::from_str_radix(::std::str::from_utf8(s).unwrap(), 16))
			.collect::<::std::result::Result<Vec<_>, _>>()?)
	}
}
