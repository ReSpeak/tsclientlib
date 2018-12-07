#![allow(unused_variables)]
use std::borrow::Cow;
use std::fmt;
use std::io::prelude::*;
use std::io::Cursor;

use byteorder::{NetworkEndian, ReadBytesExt, WriteBytesExt};
use bytes::Bytes;
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

pub(crate) struct MyBytes(Bytes);
unsafe impl ::stable_deref_trait::StableDeref for MyBytes {}
impl ::std::ops::Deref for MyBytes {
	type Target = [u8];
	fn deref(&self) -> &Self::Target { self.0.as_ref() }
}

rental! {
	mod rentals {
		use std::borrow::Cow;
		use super::*;

		#[rental(covariant)]
		pub(crate) struct Packet {
			header: MyBytes,
			content: Cow<'header, [u8]>,
		}

		#[rental]
		pub(crate) struct Command {
			#[subrental = 2]
			packet: Box<Packet>,
			data: CommandData<'packet_1>,
		}

		#[rental]
		pub(crate) struct S2CInit {
			#[subrental = 2]
			packet: Box<Packet>,
			data: S2CInitData<'packet_1>,
		}

		#[rental]
		pub(crate) struct C2SInit {
			#[subrental = 2]
			packet: Box<Packet>,
			data: C2SInitData<'packet_1>,
		}

		#[rental]
		pub(crate) struct Voice {
			#[subrental = 2]
			packet: Box<Packet>,
			data: VoiceData<'packet_1>,
		}
	}
}

pub struct CommandData<'a> {
	pub name: &'a str,
	pub static_args: Vec<(&'a str, Cow<'a, str>)>,
	pub list_args: Vec<Vec<(&'a str, Cow<'a, str>)>>,
}

pub struct Packet2 {
	inner: rentals::Packet,
	dir: Direction,
}

pub struct Header2<'a>(&'a [u8], Direction);

pub struct Command2 {
	inner: rentals::Command,
	dir: Direction,
}

/// The mac has to be `b"TS3INIT1"`.
///
/// `version` always contains the Teamspeak version as timestamp.
///
/// `timestamp` contains a current timestamp.
pub enum C2SInitData<'a> {
	Init0 { version: u32, timestamp: u32, random0: &'a [u8; 4] },
	Init2 { version: u32, random1: &'a [u8; 16], random0_r: &'a [u8; 4] },
	Init4 {
		version: u32,
		x: &'a [u8; 64],
		n: &'a [u8; 64],
		level: u32,
		random2: &'a [u8; 100],
		/// y = x ^ (2 ^ level) % n
		y: &'a [u8; 64],
		/// Has to be a `clientinitiv alpha=… beta=…` command.
		command: CommandData<'a>,
	},
}

pub enum S2CInitData<'a> {
	Init1 { random1: &'a [u8; 16], random0_r: &'a [u8; 4] },
	Init3 {
		x: &'a [u8; 64],
		n: &'a [u8; 64],
		level: u32,
		random2: &'a [u8; 100],
	},
}

pub struct S2CInit2(rentals::S2CInit);
pub struct C2SInit2(rentals::C2SInit);

pub enum VoiceData<'a> {
	C2S { id: u16, codec: CodecType, data: &'a [u8] },
	C2SWhisper {
		id: u16,
		codec: CodecType,
		channels: Vec<u64>,
		clients: Vec<u16>,
		data: &'a [u8],
	},
	/// When the `Flags::NEWPROTOCOL` is set.
	C2SWhisperNew {
		id: u16,
		codec: CodecType,
		whisper_type: u8,
		target: u8,
		target_id: u64,
		data: &'a [u8],
	},

	S2C { id: u16, from: u16, codec: CodecType, data: &'a [u8] },
	S2CWhisper { id: u16, from: u16, codec: CodecType, data: &'a [u8] },
}

pub struct Voice(rentals::Voice);

impl Packet2 {
	/// Do some sanity checks before creating the object.
	pub fn try_new(data: Bytes, dir: Direction) -> Result<Self> {
		let header_len = if dir == Direction::S2C { ::S2C_HEADER_LEN }
			else { ::C2S_HEADER_LEN };
		if data.len() < header_len {
			return Err(format_err!("Packet too short").into());
		}

		// Check packet type
		if (data[header_len - 1] & 0xf) > 8 {
			return Err(format_err!("Invalid packet type").into());
		}

		Ok(Self::new(data, dir))
	}

	/// This method expects that `data` holds a valid packet.
	///
	/// If not, further function calls may panic.
	pub fn new(data: Bytes, dir: Direction) -> Self {
		Self {
			inner: rentals::Packet::new(MyBytes(data),
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
	pub fn direction(&self) -> Direction { self.dir }

	#[inline]
	pub fn into_command(self) -> Result<Command2> {
		let p_type = self.header().packet_type();
		let dir = self.dir;

		Ok(Command2 {
			inner: rentals::Command::try_new(Box::new(self.inner), |p| {
				let s = ::std::str::from_utf8(&p.content)?;
				::commands::parse_command2(s)
			}).map_err(|e: ::rental::RentalError<Error, _>| e.0)?,
			dir,
		})
	}

	/// Parse this packet into a voice packet.
	pub fn into_voice(self) -> Result<Voice> {
		let p_type = self.header().packet_type();
		let newprotocol = self.header().flags().contains(Flags::NEWPROTOCOL);
		let dir = self.dir;
		let id = self.content().read_u16::<NetworkEndian>()?;

		Ok(Voice(rentals::Voice::try_new(Box::new(self.inner), |p| {
			let content = p.content;
			if content.len() < 5 {
				return Err(format_err!("Voice packet too short").into());
			}
			if p_type == PacketType::Voice {
				if dir == Direction::S2C {
					Ok(VoiceData::S2C {
						id,
						from: (&content[2..]).read_u16::<NetworkEndian>()?,
						codec: CodecType::from_u8(content[4])
							.ok_or_else::<Error, _>(||
								format_err!("Invalid codec").into())?,
						data: &content[5..],
					})
				} else {
					Ok(VoiceData::C2S {
						id,
						codec: CodecType::from_u8(content[3])
							.ok_or_else::<Error, _>(||
								format_err!("Invalid codec").into())?,
						data: &content[4..],
					})
				}
			} else {
				if dir == Direction::S2C {
					Ok(VoiceData::S2CWhisper {
						id,
						from: (&content[2..]).read_u16::<NetworkEndian>()?,
						codec: CodecType::from_u8(content[4])
							.ok_or_else::<Error, _>(||
								format_err!("Invalid codec").into())?,
						data: &content[5..],
					})
				} else {
					let codec = CodecType::from_u8(content[3])
						.ok_or_else::<Error, _>(||
							format_err!("Invalid codec").into())?;
					if newprotocol {
						if content.len() < 13 {
							return Err(format_err!("Voice packet too short").into());
						}
						Ok(VoiceData::C2SWhisperNew {
							id,
							codec,
							whisper_type: content[4],
							target: content[5],
							target_id: (&content[6..]).read_u64::<NetworkEndian>()?,
							data: &content[13..],
						})
					} else {
						if content.len() < 5 {
							return Err(format_err!("Voice packet too short").into());
						}
						let channel_count = content[4] as usize;
						let client_count = content[5] as usize;
						let channel_off = 5;
						let client_off = channel_off + channel_count * 8;
						let off = client_off + client_count * 2;
						if content.len() < off {
							return Err(format_err!("Voice packet too short").into());
						}

						Ok(VoiceData::C2SWhisper {
							id,
							codec,
							channels: (0..channel_count).map(|i| {
								(&content[channel_off + i * 8..])
									.read_u64::<NetworkEndian>()
							}).collect::<::std::result::Result<Vec<_>, _>>()?,
							clients: (0..client_count).map(|i| {
								(&content[client_off + i * 2..])
									.read_u16::<NetworkEndian>()
							}).collect::<::std::result::Result<Vec<_>, _>>()?,
							data: &content[off..],
						})
					}
				}
			}
		}).map_err(|e: ::rental::RentalError<Error, _>| e.0)?))
	}

	pub fn into_s2cinit(self) -> Result<S2CInit2> {
		if self.dir != Direction::S2C {
			return Err(format_err!("Wrong direction").into());
		}
		if self.header().packet_type() != PacketType::Init {
			return Err(format_err!("Not an init packet").into());
		}
		if self.header().mac() != b"TS3INIT1" {
			return Err(format_err!("Wrong init packet mac").into());
		}

		Ok(S2CInit2(rentals::S2CInit::try_new(Box::new(self.inner), |p| {
			let content = &p.content;
			if content.len() < 1 {
				return Err(format_err!("Packet too short").into());
			}

			if content[0] == 1 {
				if content.len() < 21 {
					return Err(format_err!("Packet too short").into());
				}
				Ok(S2CInitData::Init1 {
					random1: array_ref!(content, 1, 16),
					random0_r: array_ref!(content, 17, 4),
				})
			} else if content[0] == 3 {
				if content.len() < 233 {
					return Err(format_err!("Packet too short").into());
				}
				Ok(S2CInitData::Init3 {
					x: array_ref!(content, 1, 64),
					n: array_ref!(content, 65, 64),
					level: (&content[129..]).read_u32::<NetworkEndian>()?,
					random2: array_ref!(content, 133, 100),
				})
			} else {
				Err(format_err!("Invalid init step").into())
			}
		}).map_err(|e: ::rental::RentalError<Error, _>| e.0)?))
	}

	pub fn into_c2sinit(self) -> Result<C2SInit2> {
		if self.dir != Direction::C2S {
			return Err(format_err!("Wrong direction").into());
		}
		if self.header().packet_type() != PacketType::Init {
			return Err(format_err!("Not an init packet").into());
		}
		if self.header().mac() != b"TS3INIT1" {
			return Err(format_err!("Wrong init packet mac").into());
		}

		Ok(C2SInit2(rentals::C2SInit::try_new(Box::new(self.inner), |p| {
			let content = &p.content;
			if content.len() < 5 {
				return Err(format_err!("Packet too short").into());
			}

			let version = (&content[0..]).read_u32::<NetworkEndian>()?;
			if content[5] == 0 {
				if content.len() < 13 {
					return Err(format_err!("Packet too short").into());
				}
				Ok(C2SInitData::Init0 {
					version,
					timestamp: (&content[5..]).read_u32::<NetworkEndian>()?,
					random0: array_ref!(content, 9, 4),
				})
			} else if content[5] == 2 {
				if content.len() < 25 {
					return Err(format_err!("Packet too short").into());
				}
				Ok(C2SInitData::Init2 {
					version,
					random1: array_ref!(content, 5, 16),
					random0_r: array_ref!(content, 21, 4),
				})
			} else if content[5] == 4 {
				let len = 5 + 128 + 4 + 100 + 64;
				if content.len() < len + 20 {
					return Err(format_err!("Packet too short").into());
				}
				let s = ::std::str::from_utf8(&content[len..])?;
				let command = ::commands::parse_command2(s)?;
				Ok(C2SInitData::Init4 {
					version,
					x: array_ref!(content, 5, 64),
					n: array_ref!(content, 69, 64),
					level: (&content[128 + 5..]).read_u32::<NetworkEndian>()?,
					random2: array_ref!(content, 128 + 9, 100),
					y: array_ref!(content, 228 + 9, 64),
					command,
				})
			} else {
				Err(format_err!("Invalid init step").into())
			}
		}).map_err(|e: ::rental::RentalError<Error, _>| e.0)?))
	}

	#[inline]
	pub fn content(&self) -> &[u8] {
		self.inner.ref_rent(|d| &**d)
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

impl Command2 {
	#[inline]
	pub fn direction(&self) -> Direction { self.dir }
	#[inline]
	pub fn name(&self) -> &str { self.inner.ref_rent(|d| d.name) }
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
