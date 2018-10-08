use std::cell::RefCell;
use std::fmt;
use std::io::Cursor;
use std::net::IpAddr;
use std::rc::Rc;

use futures;
use futures::Sink;

use packets::Header;
use Result;

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
		// Own dumps
		Ok(s[2..]
			.split(", ")
			.map(|s| {
				if s.starts_with("0x") {
					u8::from_str_radix(&s[2..], 16)
				} else {
					u8::from_str_radix(s, 16)
				}
			}).collect::<::std::result::Result<Vec<_>, _>>()?)
	} else {
		Ok(s.as_bytes()
			.chunks(2)
			.map(|s| u8::from_str_radix(::std::str::from_utf8(s).unwrap(), 16))
			.collect::<::std::result::Result<Vec<_>, _>>()?)
	}
}

pub fn parse_packet(
	mut udp_packet: Vec<u8>,
	is_client: bool,
) -> Result<(Header, Vec<u8>)> {
	let (header, pos) = {
		let mut r = Cursor::new(udp_packet.as_slice());
		(Header::read(&!is_client, &mut r)?, r.position() as usize)
	};
	let udp_packet = udp_packet.split_off(pos);
	Ok((header, udp_packet))
}

/// A clonable sink.
pub struct MultiSink<Inner>(Rc<RefCell<Inner>>);

impl<Inner> MultiSink<Inner> {
	pub fn new(inner: Inner) -> Self {
		MultiSink(Rc::new(RefCell::new(inner)))
	}
}

impl<Inner> ::std::clone::Clone for MultiSink<Inner> {
	fn clone(&self) -> Self {
		MultiSink(self.0.clone())
	}
}

impl<I, E, Inner: Sink<SinkItem = I, SinkError = E>> Sink for MultiSink<Inner> {
	type SinkItem = I;
	type SinkError = E;

	fn start_send(
		&mut self,
		item: Self::SinkItem,
	) -> futures::StartSend<Self::SinkItem, Self::SinkError> {
		let mut inner = self.0.borrow_mut();
		inner.start_send(item)
	}

	fn poll_complete(&mut self) -> futures::Poll<(), Self::SinkError> {
		let mut inner = self.0.borrow_mut();
		inner.poll_complete()
	}
}
