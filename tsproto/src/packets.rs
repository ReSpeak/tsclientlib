#![allow(unused_variables)]
use std::borrow::Cow;
use std::fmt;
use std::io::prelude::*;
use std::io::Cursor;

use byteorder::{NetworkEndian, ReadBytesExt, WriteBytesExt};
use num::{FromPrimitive, ToPrimitive};

use commands::Command;
use utils::HexSlice;
use {Error, Result};

include!(concat!(env!("OUT_DIR"), "/packets.rs"));

#[derive(
	Debug, PartialEq, Eq, Clone, Copy, FromPrimitive, ToPrimitive, Hash,
)]
#[repr(u8)]
pub enum PacketType {
	Voice,
	VoiceWhisper,
	Command,
	CommandLow,
	Ping,
	Pong,
	Ack,
	AckLow,
	Init,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum Direction {
	/// Going from the server to the client.
	S2C,
	/// Going from the client to the server.
	C2S,
}

bitflags! {
	pub struct Flags: u8 {
		const UNENCRYPTED = 0x80;
		const COMPRESSED  = 0x40;
		const NEWPROTOCOL = 0x20;
		const FRAGMENTED  = 0x10;
	}
}

impl PacketType {
	pub fn is_command(self) -> bool {
		self == PacketType::Command || self == PacketType::CommandLow
	}
	pub fn is_voice(self) -> bool {
		self == PacketType::Voice || self == PacketType::VoiceWhisper
	}
}

#[repr(u8)]
#[derive(Debug, PartialEq, Eq, Clone, Copy, FromPrimitive, ToPrimitive)]
pub enum CodecType {
	/// Mono,   16 bit,  8 kHz, bitrate dependent on the quality setting
	SpeexNarrowband,
	/// Mono,   16 bit, 16 kHz, bitrate dependent on the quality setting
	SpeexWideband,
	/// Mono,   16 bit, 32 kHz, bitrate dependent on the quality setting
	SpeexUltrawideband,
	/// Mono,   16 bit, 48 kHz, bitrate dependent on the quality setting
	CeltMono,
	/// Mono,   16 bit, 48 kHz, bitrate dependent on the quality setting, optimized for voice
	OpusVoice,
	/// Stereo, 16 bit, 48 kHz, bitrate dependent on the quality setting, optimized for music
	OpusMusic,
}

/// Packet data, from client
pub struct UdpPacket<'a> {
	pub data: &'a [u8],
	pub from_client: bool,
}

impl<'a> UdpPacket<'a> {
	pub fn new(data: &'a [u8], from_client: bool) -> Self {
		Self { data, from_client }
	}
}

impl<'a> fmt::Debug for UdpPacket<'a> {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "UdpPacket {{ header: ")?;

		// Parse header
		if let Ok(h) =
			Header::read(&self.from_client, &mut Cursor::new(self.data))
		{
			h.fmt(f)?;
		}

		write!(f, ", raw: {:?} }}", HexSlice(self.data))?;
		Ok(())
	}
}

rental! {
	mod rentals {
		use std::borrow::Cow;
		use super::*;

		#[rental(covariant)]
		pub struct Packet {
			header: Vec<u8>,
			content: Cow<'header, [u8]>,
		}

		#[rental]
		pub struct Command {
			#[subrental = 2]
			packet: Box<Packet>,
			name: &'packet_1 str,
			static_args: Vec<Cow<'packet_1, str>>,
			dynamic_args: Vec<Vec<Cow<'packet_1, str>>>,
		}

		#[rental]
		pub struct C2SInit {
			#[subrental = 2]
			packet: Box<Packet>,
			data: C2SInitData<'packet_1>,
		}
	}
}

pub struct Packet2 {
	inner: rentals::Packet,
	dir: Direction,
}

pub struct PacketMut {
	data: Vec<u8>,
	dir: Direction,
}

pub struct Header2<'a>(&'a [u8], Direction);
pub struct HeaderMut<'a>(&'a mut [u8], Direction);

pub struct Command2 {
	inner: rentals::Command,
	dir: Direction,
}

pub enum C2SInitData<'a> {
	Init0 { version: u32, timestamp: u32, random0: &'a [u8; 4] },
}

pub struct C2SInit2 {
	inner: rentals::C2SInit,
}

impl Packet2 {
	/// Do some sanity checks before creating the object.
	pub fn try_new(data: Vec<u8>, dir: Direction) -> Result<Self> {
		let header_len = if dir == Direction::S2C { ::S2C_HEADER_LEN }
			else { ::C2S_HEADER_LEN };
		if data.len() < header_len {
			return Err(format_err!("Packet too short").into());
		}

		// Check packet type
		if (data[header_len - 1] & 0xf) > 8 {
			return Err(format_err!("Invalid packet type").into());
		}

		Ok(Self {
			inner: rentals::Packet::new(data,
				|data| if dir == Direction::S2C {
					Cow::Borrowed(&data[header_len..])
				} else {
					Cow::Borrowed(&data[header_len..])
				}),
			dir,
		})
	}

	/// This method expects that `data` holds a valid packet.
	///
	/// If not, further function calls may panic.
	pub fn new(data: Vec<u8>, dir: Direction) -> Self {
		Self {
			inner: rentals::Packet::new(data,
				|data| if dir == Direction::S2C {
					Cow::Borrowed(&data[::S2C_HEADER_LEN..])
				} else {
					Cow::Borrowed(&data[::C2S_HEADER_LEN..])
				}),
			dir,
		}
	}

	#[inline]
	pub fn header(&self) -> Header2 { Header2(self.inner.head(), self.dir) }

	#[inline]
	pub fn voice_data(&self) -> Option<&[u8]> {
		let p_type = self.header().packet_type();
		let offset = if p_type == PacketType::Voice {
			if self.dir == Direction::S2C {
				5
			} else {
				3
			}
		} else if p_type == PacketType::VoiceWhisper {
			if self.dir == Direction::S2C {
				5
			} else if self.header().flags().contains(Flags::NEWPROTOCOL) {
				13
			} else {
				5 + self.inner.rent(|c| c[4] * 8 + c[5] * 2) as usize
			}
		} else {
			return None;
		};
		Some(self.inner.ref_rent(|d| &d[offset..]))
	}
}

impl<'a> Header2<'a> {
	#[inline]
	pub fn mac(&self) -> &[u8; 8] {
		array_ref![self.0, 0, 8]
	}

	#[inline]
	pub fn packet_id(&self) -> u16 {
		(&self.0[8..10]).read_u16::<NetworkEndian>().unwrap()
	}

	#[inline]
	pub fn client_id(&self) -> Option<u16> {
		if self.1 == Direction::S2C {
			None
		} else {
			Some((&self.0[10..12]).read_u16::<NetworkEndian>().unwrap())
		}
	}

	#[inline]
	pub fn flags(&self) -> Flags {
		let off = if self.1 == Direction::S2C { 10 } else { 12 };
		Flags::from_bits(self.0[off] & 0xf0).unwrap()
	}

	#[inline]
	pub fn packet_type(&self) -> PacketType {
		let off = if self.1 == Direction::S2C { 10 } else { 12 };
		PacketType::from_u8(self.0[off] & 0xf0).unwrap()
	}
}

impl<'a> HeaderMut<'a> {
	#[inline]
	fn header<'b: 'a>(&'b self) -> Header2<'a> {
		Header2(self.0, self.1)
	}
}

impl Packet {
	pub fn new(header: Header, data: Data) -> Packet {
		Packet { header, data }
	}
}

impl Default for Header {
	fn default() -> Self {
		Header {
			mac: Default::default(),
			p_id: 0,
			c_id: None,
			p_type: 0,
		}
	}
}

impl Header {
	pub fn new(p_type: PacketType) -> Header {
		let mut h = Header::default();
		h.set_type(p_type);
		h
	}

	pub fn get_p_type(&self) -> u8 {
		self.p_type
	}
	pub fn set_p_type(&mut self, p_type: u8) {
		assert!((p_type & 0xf) <= 8);
		self.p_type = p_type;
	}

	/// `true` if the packet is not encrypted.
	pub fn get_unencrypted(&self) -> bool {
		(self.get_p_type() & 0x80) != 0
	}
	/// `true` if the packet is compressed.
	pub fn get_compressed(&self) -> bool {
		(self.get_p_type() & 0x40) != 0
	}
	pub fn get_newprotocol(&self) -> bool {
		(self.get_p_type() & 0x20) != 0
	}
	/// `true` for the first and last packet of a compressed series of packets.
	pub fn get_fragmented(&self) -> bool {
		(self.get_p_type() & 0x10) != 0
	}

	pub fn set_unencrypted(&mut self, value: bool) {
		let p_type = self.get_p_type();
		if value {
			self.set_p_type(p_type | 0x80);
		} else {
			self.set_p_type(p_type & !0x80);
		}
	}
	pub fn set_compressed(&mut self, value: bool) {
		let p_type = self.get_p_type();
		if value {
			self.set_p_type(p_type | 0x40);
		} else {
			self.set_p_type(p_type & !0x40);
		}
	}
	pub fn set_newprotocol(&mut self, value: bool) {
		let p_type = self.get_p_type();
		if value {
			self.set_p_type(p_type | 0x20);
		} else {
			self.set_p_type(p_type & !0x20);
		}
	}
	pub fn set_fragmented(&mut self, value: bool) {
		let p_type = self.get_p_type();
		if value {
			self.set_p_type(p_type | 0x10);
		} else {
			self.set_p_type(p_type & !0x10);
		}
	}

	pub fn get_type(&self) -> PacketType {
		PacketType::from_u8(self.get_p_type() & 0xf).unwrap()
	}
	pub fn set_type(&mut self, t: PacketType) {
		let p_type = self.get_p_type();
		self.set_p_type((p_type & 0xf0) | t.to_u8().unwrap());
	}

	pub fn write_meta(&self, w: &mut Write) -> Result<()> {
		w.write_u16::<NetworkEndian>(self.p_id)?;
		if let Some(c_id) = self.c_id {
			w.write_u16::<NetworkEndian>(c_id)?;
		}
		w.write_u8(self.p_type)?;
		Ok(())
	}
}
