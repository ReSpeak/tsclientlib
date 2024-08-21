#![allow(clippy::new_ret_no_self)]
use std::convert::TryInto;
use std::io::prelude::*;
use std::{fmt, io, str};

use base64::prelude::*;
use bitflags::bitflags;
use num_derive::{FromPrimitive, ToPrimitive};
use num_traits::{FromPrimitive as _, ToPrimitive as _};
use omnom::{ReadExt, WriteExt};
use serde::{Deserialize, Serialize};

use crate::{Error, HexSlice, Result};

#[derive(
	Clone, Copy, Debug, Deserialize, Eq, FromPrimitive, Hash, PartialEq, ToPrimitive, Serialize,
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

#[derive(Clone, Copy, Deserialize, Debug, Eq, PartialEq, Hash, Serialize)]
pub enum Direction {
	/// Going from the server to the client.
	S2C,
	/// Going from the client to the server.
	C2S,
}

bitflags! {
	#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
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
	pub fn is_ack(self) -> bool {
		self == PacketType::Ack || self == PacketType::AckLow || self == PacketType::Pong
	}
	pub fn is_voice(self) -> bool { self == PacketType::Voice || self == PacketType::VoiceWhisper }
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

macro_rules! create_buf {
	($($name:ident, $borrow_name:ident, $convert:ident);*) => {
		mod rentals {
			use super::*;

			$(self_cell::self_cell!(
				pub struct $name {
					owner: Vec<u8>,
					#[covariant]
					dependent: $borrow_name,
				}

				impl {Debug}
			);)*
		}

		$(
		#[derive(Debug)]
		pub struct $name(rentals::$name);

		impl $name {
			/// `InPacket::try_new` is not checked and must succeed. Otherwise, it
			/// panics.
			#[inline]
			pub fn try_new(direction: Direction, data: Vec<u8>) -> Result<Self> {
				Ok(Self(rentals::$name::try_new(data, |data| InPacket::new(direction, data).$convert())?))
			}
			#[inline]
			pub fn raw_data(&self) -> &[u8] { self.0.borrow_owner().as_slice() }
			#[inline]
			pub fn data(&self) -> &$borrow_name { self.0.borrow_dependent() }
			#[inline]
			pub fn into_buffer(self) -> Vec<u8> { self.0.into_owner() }
		}
		)*
	}
}

create_buf!(InAudioBuf, InAudio, into_audio;
	InCommandBuf, InCommand, into_command;
	InC2SInitBuf, InC2SInit, into_c2sinit;
	InS2CInitBuf, InS2CInit, into_s2cinit);

/// Used for debugging.
#[derive(Clone)]
pub struct InUdpPacket<'a>(pub InPacket<'a>);

impl<'a> InUdpPacket<'a> {
	pub fn new(packet: InPacket<'a>) -> Self { Self(packet) }
}

#[derive(Clone)]
pub struct InHeader<'a> {
	direction: Direction,
	data: &'a [u8],
}

#[derive(Clone)]
pub struct InPacket<'a> {
	header: InHeader<'a>,
	content: &'a [u8],
}

#[derive(Clone, Debug)]
pub struct InCommand<'a> {
	packet: InPacket<'a>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Hash, Serialize)]
pub struct OutUdpPacket {
	generation_id: u32,
	data: OutPacket,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Hash, Serialize)]
pub struct OutPacket {
	dir: Direction,
	data: Vec<u8>,
}

/// The mac has to be `b"TS3INIT1"`.
///
/// `version` always contains the Teamspeak version as timestamp.
///
/// `timestamp` contains a current timestamp.
#[derive(Clone)]
pub enum C2SInitData<'a> {
	Init0 {
		version: u32,
		timestamp: u32,
		random0: &'a [u8; 4],
	},
	Init2 {
		version: u32,
		random1: &'a [u8; 16],
		random0_r: &'a [u8; 4],
	},
	Init4 {
		version: u32,
		x: &'a [u8; 64],
		n: &'a [u8; 64],
		level: u32,
		random2: &'a [u8; 100],
		/// y = x ^ (2 ^ level) % n
		y: &'a [u8; 64],
		/// Has to be a `clientinitiv alpha=… omega=…` command.
		command: &'a [u8],
	},
}

#[derive(Clone)]
pub enum S2CInitData<'a> {
	Init1 { random1: &'a [u8; 16], random0_r: &'a [u8; 4] },
	Init3 { x: &'a [u8; 64], n: &'a [u8; 64], level: u32, random2: &'a [u8; 100] },
	Init127 {},
}

#[derive(Clone, Debug)]
pub struct InS2CInit<'a> {
	packet: InPacket<'a>,
	data: S2CInitData<'a>,
}

#[derive(Clone, Debug)]
pub struct InC2SInit<'a> {
	packet: InPacket<'a>,
	data: C2SInitData<'a>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AudioData<'a> {
	C2S {
		id: u16,
		codec: CodecType,
		data: &'a [u8],
	},
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

	S2C {
		id: u16,
		from: u16,
		codec: CodecType,
		data: &'a [u8],
	},
	S2CWhisper {
		id: u16,
		from: u16,
		codec: CodecType,
		data: &'a [u8],
	},
}

#[derive(Clone)]
pub struct InAudio<'a> {
	packet: InPacket<'a>,
	data: AudioData<'a>,
}

#[must_use]
pub struct OutCommand(pub OutPacket);
pub struct OutC2SInit0;
pub struct OutC2SInit2;
pub struct OutC2SInit4;
pub struct OutS2CInit1;
pub struct OutS2CInit3;
pub struct OutAck;
pub struct OutAudio;

/// A helper to escape data while writing into a buffer.
struct EscapedWriter<'a>(&'a mut Vec<u8>);

impl Direction {
	#[inline]
	pub fn reverse(self) -> Self {
		match self {
			Direction::S2C => Direction::C2S,
			Direction::C2S => Direction::S2C,
		}
	}
}

impl<'a> InPacket<'a> {
	/// Do some sanity checks before creating the object.
	#[inline]
	pub fn try_new(direction: Direction, data: &'a [u8]) -> Result<Self> {
		let header_len =
			if direction == Direction::S2C { crate::S2C_HEADER_LEN } else { crate::C2S_HEADER_LEN };
		if data.len() < header_len {
			return Err(Error::PacketTooShort(data.len()));
		}

		// Check packet type
		let p_type = data[header_len - 1] & 0xf;
		if p_type > 8 {
			return Err(Error::UnknownPacketType(p_type));
		}

		Ok(Self::new(direction, data))
	}

	/// This method expects that `data` holds a valid packet.
	///
	/// If not, further function calls may panic.
	#[inline]
	pub fn new(direction: Direction, data: &'a [u8]) -> Self {
		let header = InHeader::new(direction, data);
		let header_len = header.data.len();

		Self { header, content: &data[header_len..] }
	}

	#[inline]
	pub fn header(&self) -> &InHeader<'a> { &self.header }
	#[inline]
	pub fn content(&self) -> &[u8] { self.content }

	/// Get the acknowledged packet id if this is an ack packet.
	#[inline]
	pub fn ack_packet(&self) -> Result<Option<u16>> {
		let p_type = self.header().packet_type();
		if p_type.is_ack() {
			Ok(Some(
				self.content()
					.read_be()
					.map_err(|_| Error::PacketContentTooShort(self.content().len()))?,
			))
		} else if p_type == PacketType::Init {
			if self.header.direction == Direction::S2C {
				Ok(Some(
					self.content
						.first()
						.ok_or_else(|| Error::PacketContentTooShort(self.content().len()))
						.and_then(|i| match u16::from(*i) {
							1 => Ok(0),
							3 => Ok(2),
							127 => Ok(2), // Have to restart sending Init0, remove Init2 anyway
							_ => Err(Error::InvalidInitStep(*i)),
						})?,
				))
			} else {
				Ok(self
					.content
					.get(4)
					.ok_or_else(|| Error::PacketContentTooShort(self.content().len()))
					.and_then(|i| match u16::from(*i) {
						0 => Ok(None),
						2 => Ok(Some(1)),
						4 => Ok(Some(3)),
						_ => Err(Error::InvalidInitStep(*i)),
					})?)
			}
		} else {
			Ok(None)
		}
	}

	/// Parse this packet into a voice packet.
	pub fn into_audio(self) -> Result<InAudio<'a>> {
		let p_type = self.header().packet_type();
		let newprotocol = self.header().flags().contains(Flags::NEWPROTOCOL);
		let data = AudioData::parse(p_type, newprotocol, self.header.direction, self.content)?;

		Ok(InAudio { packet: self, data })
	}

	/// Put this packet into a command packet.
	pub fn into_command(self) -> Result<InCommand<'a>> { Ok(InCommand { packet: self }) }

	pub fn into_s2cinit(self) -> Result<InS2CInit<'a>> {
		if self.header.direction != Direction::S2C {
			return Err(Error::WrongDirection);
		}
		let p_type = self.header().packet_type();
		if p_type != PacketType::Init {
			return Err(Error::WrongPacketType(p_type));
		}
		let mac = self.header().mac();
		if mac != b"TS3INIT1" {
			return Err(Error::WrongInitMac(mac.to_vec()));
		}

		if self.content.is_empty() {
			return Err(Error::PacketContentTooShort(self.content.len()));
		}

		let data;
		if self.content[0] == 1 {
			if self.content.len() < 21 {
				return Err(Error::PacketContentTooShort(self.content.len()));
			}
			data = S2CInitData::Init1 {
				random1: (&self.content[1..17]).try_into().unwrap(),
				random0_r: (&self.content[17..21]).try_into().unwrap(),
			};
		} else if self.content[0] == 3 {
			if self.content.len() < 233 {
				return Err(Error::PacketContentTooShort(self.content.len()));
			}
			data = S2CInitData::Init3 {
				x: (&self.content[1..65]).try_into().unwrap(),
				n: (&self.content[65..129]).try_into().unwrap(),
				level: (&self.content[129..]).read_be()?,
				random2: (&self.content[133..233]).try_into().unwrap(),
			};
		} else if self.content[0] == 127 {
			data = S2CInitData::Init127 {};
		} else {
			return Err(Error::InvalidInitStep(self.content[0]));
		}

		Ok(InS2CInit { packet: self, data })
	}

	pub fn into_c2sinit(self) -> Result<InC2SInit<'a>> {
		if self.header.direction != Direction::C2S {
			return Err(Error::WrongDirection);
		}
		let p_type = self.header().packet_type();
		if p_type != PacketType::Init {
			return Err(Error::WrongPacketType(p_type));
		}
		let mac = self.header().mac();
		if mac != b"TS3INIT1" {
			return Err(Error::WrongInitMac(mac.to_vec()));
		}

		if self.content.len() < 5 {
			return Err(Error::PacketContentTooShort(self.content.len()));
		}

		let data;
		let version = (&self.content[0..]).read_be()?;
		if self.content[4] == 0 {
			if self.content.len() < 13 {
				return Err(Error::PacketContentTooShort(self.content.len()));
			}
			data = C2SInitData::Init0 {
				version,
				timestamp: (&self.content[5..]).read_be()?,
				random0: (&self.content[9..13]).try_into().unwrap(),
			};
		} else if self.content[4] == 2 {
			if self.content.len() < 25 {
				return Err(Error::PacketContentTooShort(self.content.len()));
			}
			data = C2SInitData::Init2 {
				version,
				random1: (&self.content[5..21]).try_into().unwrap(),
				random0_r: (&self.content[21..25]).try_into().unwrap(),
			};
		} else if self.content[4] == 4 {
			let len = 5 + 128 + 4 + 100 + 64;
			if self.content.len() < len + 20 {
				return Err(Error::PacketContentTooShort(self.content.len()));
			}
			data = C2SInitData::Init4 {
				version,
				x: (&self.content[5..69]).try_into().unwrap(),
				n: (&self.content[69..133]).try_into().unwrap(),
				level: (&self.content[133..]).read_be()?,
				random2: (&self.content[137..237]).try_into().unwrap(),
				y: (&self.content[237..301]).try_into().unwrap(),
				command: &self.content[len..],
			};
		} else {
			return Err(Error::InvalidInitStep(self.content[0]));
		}

		Ok(InC2SInit { packet: self, data })
	}
}

impl fmt::Debug for InPacket<'_> {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "Packet({:?}", self.header())?;
		let mut success = false;
		match self.header.packet_type() {
			PacketType::Voice | PacketType::VoiceWhisper => {
				if let Ok(packet) = self.clone().into_audio() {
					success = true;
					write!(f, ", {:?}", packet)?;
				}
			}
			PacketType::Command | PacketType::CommandLow => {
				if let Ok(packet) = str::from_utf8(self.content()) {
					success = true;
					write!(f, ", {:?}", packet)?;
				}
			}
			PacketType::Ping | PacketType::Pong | PacketType::Ack | PacketType::AckLow => {
				success = true;
				if !self.content().is_empty() {
					write!(f, ", 0x")?;
				}
				for b in self.content() {
					write!(f, "{:02x}", b)?;
				}
			}
			PacketType::Init => {
				if self.header.direction == Direction::C2S {
					if let Ok(packet) = self.clone().into_c2sinit() {
						success = true;
						write!(f, ", {:?}", packet.data)?;
					}
				} else if let Ok(packet) = self.clone().into_s2cinit() {
					success = true;
					write!(f, ", {:?}", packet.data)?;
				}
			}
		}

		if !success {
			write!(f, ", failed to parse, content: {})", HexSlice(self.content()))?;
		}

		write!(f, ")")?;
		Ok(())
	}
}

impl fmt::Debug for InUdpPacket<'_> {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "Packet({:?}, content: {})", self.0.header(), HexSlice(self.0.content()))?;
		Ok(())
	}
}

impl fmt::Debug for InHeader<'_> {
	#[rustfmt::skip]
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "Header(")?;
		if self.mac() != &[0; 8] {
			write!(f, "mac: {}, ", HexSlice(self.mac()))?;
		}
		write!(f, "id: {:#x}, ", self.packet_id())?;
		if let Some(c_id) = self.client_id() {
			write!(f, "c_id: {:#x}, ", c_id)?;
		}
		write!(f, "{:?}, ", self.packet_type())?;
		write!(f, "")?;
		let flags = self.flags();
		write!(f, "{}", if flags.contains(Flags::UNENCRYPTED) { "u" } else { "-" })?;
		write!(f, "{}", if flags.contains(Flags::COMPRESSED) { "c" } else { "-" })?;
		write!(f, "{}", if flags.contains(Flags::NEWPROTOCOL) { "n" } else { "-" })?;
		write!(f, "{}", if flags.contains(Flags::FRAGMENTED) { "f" } else { "-" })?;

		write!(f, ")")?;
		Ok(())
	}
}

impl<'a> InHeader<'a> {
	#[inline]
	pub fn new(direction: Direction, data: &'a [u8]) -> Self {
		let header_len =
			if direction == Direction::S2C { crate::S2C_HEADER_LEN } else { crate::C2S_HEADER_LEN };

		Self { direction, data: &data[..header_len] }
	}

	/// The offset to the packet type.
	#[inline]
	fn get_off(&self) -> usize { if self.direction == Direction::S2C { 10 } else { 12 } }

	#[inline]
	pub fn direction(&self) -> Direction { self.direction }
	#[inline]
	pub fn data(&self) -> &'a [u8] { self.data }
	#[inline]
	pub fn mac(&self) -> &'a [u8; 8] { (&self.data[..8]).try_into().unwrap() }
	#[inline]
	pub fn packet_id(&self) -> u16 { (&self.data[8..10]).read_be().unwrap() }

	#[inline]
	pub fn client_id(&self) -> Option<u16> {
		if self.direction == Direction::S2C {
			None
		} else {
			Some((&self.data[10..12]).read_be().unwrap())
		}
	}

	#[inline]
	pub fn flags(&self) -> Flags { Flags::from_bits(self.data[self.get_off()] & 0xf0).unwrap() }

	#[inline]
	pub fn packet_type(&self) -> PacketType {
		PacketType::from_u8(self.data[self.get_off()] & 0xf).unwrap()
	}

	pub fn get_meta(&self) -> &'a [u8] { &self.data[8..] }
}

impl C2SInitData<'_> {
	pub fn get_step(&self) -> u8 {
		match self {
			C2SInitData::Init0 { .. } => 0,
			C2SInitData::Init2 { .. } => 2,
			C2SInitData::Init4 { .. } => 4,
		}
	}
}

impl<'a> fmt::Debug for C2SInitData<'a> {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		match self {
			C2SInitData::Init0 { .. } => write!(f, "Init0"),
			C2SInitData::Init2 { .. } => write!(f, "Init2"),
			C2SInitData::Init4 { level, command, .. } => {
				write!(f, "Init4(level: {}, ", level)?;
				if let Ok(s) = str::from_utf8(command) {
					write!(f, "{:?}", s)?;
				} else {
					write!(f, "{}", HexSlice(command))?;
				}
				write!(f, ")")?;
				Ok(())
			}
		}
	}
}

impl S2CInitData<'_> {
	pub fn get_step(&self) -> u8 {
		match self {
			S2CInitData::Init1 { .. } => 1,
			S2CInitData::Init3 { .. } => 3,
			S2CInitData::Init127 { .. } => 127,
		}
	}
}

impl<'a> fmt::Debug for S2CInitData<'a> {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		match self {
			S2CInitData::Init1 { .. } => write!(f, "Init1"),
			S2CInitData::Init3 { level, .. } => write!(f, "Init3(level: {})", level),
			S2CInitData::Init127 { .. } => write!(f, "Init127"),
		}
	}
}

impl<'a> InCommand<'a> {
	#[inline]
	pub fn packet(&self) -> &InPacket<'a> { &self.packet }
}

impl<'a> AudioData<'a> {
	pub fn parse(
		p_type: PacketType, newprotocol: bool, dir: Direction, content: &'a [u8],
	) -> Result<Self> {
		let id = (&content[..]).read_be()?;
		if p_type == PacketType::Voice {
			if dir == Direction::S2C {
				if content.len() < 5 {
					return Err(Error::PacketContentTooShort(content.len()));
				}
				Ok(AudioData::S2C {
					id,
					from: (&content[2..]).read_be()?,
					codec: CodecType::from_u8(content[4])
						.ok_or_else(|| Error::InvalidCodec(content[4]))?,
					data: &content[5..],
				})
			} else {
				if content.len() < 3 {
					return Err(Error::PacketContentTooShort(content.len()));
				}
				Ok(AudioData::C2S {
					id,
					codec: CodecType::from_u8(content[2])
						.ok_or_else(|| Error::InvalidCodec(content[4]))?,
					data: &content[3..],
				})
			}
		} else if dir == Direction::S2C {
			if content.len() < 5 {
				return Err(Error::PacketContentTooShort(content.len()));
			}
			Ok(AudioData::S2CWhisper {
				id,
				from: (&content[2..]).read_be()?,
				codec: CodecType::from_u8(content[4])
					.ok_or_else(|| Error::InvalidCodec(content[4]))?,
				data: &content[5..],
			})
		} else {
			if content.len() < 3 {
				return Err(Error::PacketContentTooShort(content.len()));
			}
			let codec =
				CodecType::from_u8(content[2]).ok_or_else(|| Error::InvalidCodec(content[4]))?;
			if newprotocol {
				if content.len() < 14 {
					return Err(Error::PacketContentTooShort(content.len()));
				}
				Ok(AudioData::C2SWhisperNew {
					id,
					codec,
					whisper_type: content[3],
					target: content[4],
					target_id: (&content[5..]).read_be()?,
					data: &content[13..],
				})
			} else {
				if content.len() < 5 {
					return Err(Error::PacketContentTooShort(content.len()));
				}
				let channel_count = content[3] as usize;
				let client_count = content[4] as usize;
				let channel_off = 5;
				let client_off = channel_off + channel_count * 8;
				let off = client_off + client_count * 2;
				if content.len() < off {
					return Err(Error::PacketContentTooShort(content.len()));
				}

				Ok(AudioData::C2SWhisper {
					id,
					codec,
					channels: (0..channel_count)
						.map(|i| (&content[channel_off + i * 8..]).read_be())
						.collect::<::std::result::Result<Vec<_>, _>>()?,
					clients: (0..client_count)
						.map(|i| (&content[client_off + i * 2..]).read_be())
						.collect::<::std::result::Result<Vec<_>, _>>()?,
					data: &content[off..],
				})
			}
		}
	}

	#[inline]
	pub fn direction(&self) -> Direction {
		match self {
			AudioData::C2S { .. } => Direction::C2S,
			AudioData::C2SWhisper { .. } => Direction::C2S,
			AudioData::C2SWhisperNew { .. } => Direction::C2S,
			AudioData::S2C { .. } => Direction::S2C,
			AudioData::S2CWhisper { .. } => Direction::S2C,
		}
	}

	#[inline]
	pub fn packet_type(&self) -> PacketType {
		match self {
			AudioData::C2S { .. } => PacketType::Voice,
			AudioData::C2SWhisper { .. } => PacketType::VoiceWhisper,
			AudioData::C2SWhisperNew { .. } => PacketType::VoiceWhisper,
			AudioData::S2C { .. } => PacketType::Voice,
			AudioData::S2CWhisper { .. } => PacketType::VoiceWhisper,
		}
	}

	#[inline]
	pub fn codec(&self) -> CodecType {
		match self {
			AudioData::C2S { codec, .. } => *codec,
			AudioData::C2SWhisper { codec, .. } => *codec,
			AudioData::C2SWhisperNew { codec, .. } => *codec,
			AudioData::S2C { codec, .. } => *codec,
			AudioData::S2CWhisper { codec, .. } => *codec,
		}
	}

	#[inline]
	pub fn id(&self) -> u16 {
		match self {
			AudioData::C2S { id, .. } => *id,
			AudioData::C2SWhisper { id, .. } => *id,
			AudioData::C2SWhisperNew { id, .. } => *id,
			AudioData::S2C { id, .. } => *id,
			AudioData::S2CWhisper { id, .. } => *id,
		}
	}

	#[inline]
	pub fn flags(&self) -> Flags {
		match self {
			AudioData::C2S { .. } => Flags::empty(),
			AudioData::C2SWhisper { .. } => Flags::empty(),
			AudioData::C2SWhisperNew { .. } => Flags::NEWPROTOCOL,
			AudioData::S2C { .. } => Flags::empty(),
			AudioData::S2CWhisper { .. } => Flags::empty(),
		}
	}

	#[inline]
	pub fn data(&self) -> &[u8] {
		match self {
			AudioData::C2S { data, .. } => data,
			AudioData::C2SWhisper { data, .. } => data,
			AudioData::C2SWhisperNew { data, .. } => data,
			AudioData::S2C { data, .. } => data,
			AudioData::S2CWhisper { data, .. } => data,
		}
	}
}

impl<'a> InS2CInit<'a> {
	#[inline]
	pub fn packet(&self) -> &InPacket<'a> { &self.packet }
	#[inline]
	pub fn data(&self) -> &S2CInitData { &self.data }
}

impl<'a> InC2SInit<'a> {
	#[inline]
	pub fn packet(&self) -> &InPacket<'a> { &self.packet }
	#[inline]
	pub fn data(&self) -> &C2SInitData { &self.data }
}

impl<'a> InAudio<'a> {
	#[inline]
	pub fn packet(&self) -> &InPacket<'a> { &self.packet }
	#[inline]
	pub fn data(&self) -> &AudioData { &self.data }
}

impl fmt::Debug for InAudio<'_> {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		match &self.data {
			AudioData::C2S { id, codec, data } => {
				write!(f, "Audio(id: {}, {:?}, {})", id, codec, HexSlice(data))?;
			}
			AudioData::C2SWhisper { id, codec, channels, clients, data } => {
				write!(
					f,
					"Whisper(id: {}, {:?}, channels: {:?}, clients: {:?}, {})",
					id,
					codec,
					channels,
					clients,
					HexSlice(data)
				)?;
			}
			AudioData::C2SWhisperNew { id, codec, whisper_type, target, target_id, data } => {
				write!(
					f,
					"WhisperNew(id: {}, {:?}, type: {}, target: {}, target_id: {}, {})",
					id,
					codec,
					whisper_type,
					target,
					target_id,
					HexSlice(data)
				)?;
			}
			AudioData::S2C { id, from, codec, data } => {
				write!(f, "Audio(id: {}, from: {}, {:?}, {})", id, from, codec, HexSlice(data))?;
			}
			AudioData::S2CWhisper { id, from, codec, data } => {
				write!(f, "Whisper(id: {}, from: {}, {:?}, {})", id, from, codec, HexSlice(data))?;
			}
		}

		Ok(())
	}
}

impl OutPacket {
	#[inline]
	pub fn new(
		mac: [u8; 8], packet_id: u16, client_id: Option<u16>, flags: Flags, packet_type: PacketType,
	) -> Self {
		let dir = if client_id.is_some() { Direction::C2S } else { Direction::S2C };
		let mut res = Self::new_with_dir(dir, flags, packet_type);
		res.data[..8].copy_from_slice(&mac);
		res.packet_id(packet_id);
		if let Some(cid) = client_id {
			res.client_id(cid);
		}
		res
	}

	/// Fill packet with known data. The rest gets filled by `packet_codec`.
	#[inline]
	pub fn new_with_dir(dir: Direction, flags: Flags, packet_type: PacketType) -> Self {
		let data =
			vec![
				0;
				if dir == Direction::S2C { crate::S2C_HEADER_LEN } else { crate::C2S_HEADER_LEN }
			];
		let mut res = Self { dir, data };
		res.flags(flags);
		res.packet_type(packet_type);
		res
	}

	#[inline]
	pub fn new_from_data(dir: Direction, data: Vec<u8>) -> Self { Self { dir, data } }

	#[inline]
	pub fn into_vec(self) -> Vec<u8> { self.data }

	#[inline]
	fn content_offset(&self) -> usize {
		if self.dir == Direction::S2C { crate::S2C_HEADER_LEN } else { crate::C2S_HEADER_LEN }
	}

	#[inline]
	pub fn data(&self) -> &[u8] { &self.data }
	#[inline]
	pub fn data_mut(&mut self) -> &mut Vec<u8> { &mut self.data }
	#[inline]
	pub fn content(&self) -> &[u8] { &self.data[self.content_offset()..] }
	#[inline]
	pub fn content_mut(&mut self) -> &mut [u8] {
		let off = self.content_offset();
		&mut self.data[off..]
	}
	#[inline]
	pub fn direction(&self) -> Direction { self.dir }
	#[inline]
	pub fn header(&self) -> InHeader { InHeader { direction: self.dir, data: self.header_bytes() } }
	#[inline]
	pub fn header_bytes(&self) -> &[u8] { &self.data[..self.content_offset()] }
	#[inline]
	pub fn packet(&self) -> InPacket { InPacket::new(self.dir, &self.data) }

	#[inline]
	pub fn mac(&mut self) -> &mut [u8; 8] { (&mut self.data[0..8]).try_into().unwrap() }
	#[inline]
	pub fn packet_id(&mut self, packet_id: u16) {
		(&mut self.data[8..10]).write_be(packet_id).unwrap();
	}
	#[inline]
	pub fn client_id(&mut self, client_id: u16) {
		assert_eq!(
			self.dir,
			Direction::C2S,
			"Client id is only valid for client to server packets"
		);
		(&mut self.data[10..12]).write_be(client_id).unwrap();
	}
	#[inline]
	pub fn flags(&mut self, flags: Flags) {
		let off = self.header().get_off();
		self.data[off] = (self.data[off] & 0xf) | flags.bits();
	}
	#[inline]
	pub fn packet_type(&mut self, packet_type: PacketType) {
		let off = self.header().get_off();
		self.data[off] = (self.data[off] & 0xf0) | packet_type.to_u8().unwrap();
	}
}

impl OutUdpPacket {
	#[inline]
	pub fn new(generation_id: u32, data: OutPacket) -> Self { Self { generation_id, data } }

	#[inline]
	pub fn generation_id(&self) -> u32 { self.generation_id }
	#[inline]
	pub fn data(&self) -> &OutPacket { &self.data }

	#[inline]
	pub fn packet_id(&self) -> u16 {
		if self.packet_type() == PacketType::Init {
			if self.data.dir == Direction::S2C {
				u16::from(self.data.content()[0])
			} else {
				u16::from(self.data.content()[4])
			}
		} else {
			self.data.header().packet_id()
		}
	}
	#[inline]
	pub fn packet_type(&self) -> PacketType { self.data.header().packet_type() }
}

impl io::Write for EscapedWriter<'_> {
	fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
		self.0.reserve(buf.len());
		for c in buf {
			match c {
				b'\x0b' => self.0.extend_from_slice(b"\\v"),
				b'\x0c' => self.0.extend_from_slice(b"\\f"),
				b'\\' => self.0.extend_from_slice(b"\\\\"),
				b'\t' => self.0.extend_from_slice(b"\\t"),
				b'\r' => self.0.extend_from_slice(b"\\r"),
				b'\n' => self.0.extend_from_slice(b"\\n"),
				b'|' => self.0.extend_from_slice(b"\\p"),
				b' ' => self.0.extend_from_slice(b"\\s"),
				b'/' => self.0.extend_from_slice(b"\\/"),
				c => self.0.push(*c),
			}
		}
		Ok(buf.len())
	}

	#[inline]
	fn flush(&mut self) -> io::Result<()> { Ok(()) }
}

impl OutCommand {
	#[inline]
	pub fn new(dir: Direction, flags: Flags, p_type: PacketType, name: &str) -> Self {
		let mut res = Self(OutPacket::new_with_dir(dir, Flags::empty(), p_type));
		res.0.flags(flags);
		res.0.data.extend_from_slice(name.as_bytes());
		res
	}

	/// For binary arguments. The value will still be escaped.
	#[inline]
	pub fn write_bin_arg(&mut self, name: &str, value: &[u8]) {
		if self.0.content().last() != Some(&b'|') && !self.0.content().is_empty() {
			self.0.data.push(b' ');
		}
		self.0.data.extend_from_slice(name.as_bytes());
		if !value.is_empty() {
			self.0.data.push(b'=');
			EscapedWriter(&mut self.0.data).write_all(value).unwrap();
		}
	}

	/// The value will be formatted and escaped.
	#[inline]
	pub fn write_arg(&mut self, name: &str, value: &dyn fmt::Display) {
		if self.0.content().last() != Some(&b'|') && !self.0.content().is_empty() {
			self.0.data.push(b' ');
		}
		self.0.data.extend_from_slice(name.as_bytes());
		self.0.data.push(b'=');
		let len = self.0.data.len();
		write!(EscapedWriter(&mut self.0.data), "{}", value).unwrap();
		if self.0.data.len() == len {
			// Nothing was written, remove =
			self.0.data.pop();
		}
	}

	/// Adds a pipe symbol `|` to the command.
	#[inline]
	pub fn start_new_part(&mut self) { self.0.data.push(b'|'); }
	#[inline]
	pub fn into_packet(self) -> OutPacket { self.0 }
}

impl OutC2SInit0 {
	pub fn new(version: u32, timestamp: u32, random0: [u8; 4]) -> OutPacket {
		let mut res = OutPacket::new_with_dir(Direction::C2S, Flags::empty(), PacketType::Init);
		res.mac().copy_from_slice(b"TS3INIT1");
		res.packet_id(0x65);
		let content = res.data_mut();
		content.write_be(version).unwrap();
		content.write_be(0u8).unwrap();
		content.write_be(timestamp).unwrap();
		content.write_all(&random0).unwrap();
		// Reserved
		content.write_all(&[0u8; 8]).unwrap();
		res
	}
}

impl OutC2SInit2 {
	pub fn new(version: u32, random1: &[u8; 16], random0_r: [u8; 4]) -> OutPacket {
		let mut res = OutPacket::new_with_dir(Direction::C2S, Flags::empty(), PacketType::Init);
		res.mac().copy_from_slice(b"TS3INIT1");
		res.packet_id(0x65);
		let content = res.data_mut();
		content.write_be(version).unwrap();
		content.write_be(2u8).unwrap();
		content.write_all(random1).unwrap();
		content.write_all(&random0_r).unwrap();
		res
	}
}

impl OutC2SInit4 {
	pub fn new(
		version: u32, x: &[u8; 64], n: &[u8; 64], level: u32, random2: &[u8; 100], y: &[u8; 64],
		alpha: &[u8], omega: &[u8], ip: &str,
	) -> OutPacket {
		let mut res = OutPacket::new_with_dir(Direction::C2S, Flags::empty(), PacketType::Init);
		res.mac().copy_from_slice(b"TS3INIT1");
		res.packet_id(0x65);
		let content = res.data_mut();
		content.write_be(version).unwrap();
		content.write_be(4u8).unwrap();
		content.write_all(x).unwrap();
		content.write_all(n).unwrap();
		content.write_be(level).unwrap();
		content.write_all(random2).unwrap();
		content.write_all(y).unwrap();
		let ip = if ip.is_empty() { String::new() } else { format!("={}", ip) };
		content
			.write_all(
				format!(
					"clientinitiv alpha={} omega={} ot=1 ip{}",
					BASE64_STANDARD.encode(alpha),
					BASE64_STANDARD.encode(omega),
					ip
				)
				.as_bytes(),
			)
			.unwrap();
		res
	}
}

impl OutS2CInit1 {
	pub fn new(random1: &[u8; 16], random0_r: [u8; 4]) -> OutPacket {
		let mut res = OutPacket::new_with_dir(Direction::S2C, Flags::empty(), PacketType::Init);
		res.mac().copy_from_slice(b"TS3INIT1");
		res.packet_id(0x65);
		let content = res.data_mut();
		content.write_be(1u8).unwrap();
		content.write_all(random1).unwrap();
		content.write_all(&random0_r).unwrap();
		res
	}
}

impl OutS2CInit3 {
	pub fn new(x: &[u8; 64], n: &[u8; 64], level: u32, random2: &[u8; 100]) -> OutPacket {
		let mut res = OutPacket::new_with_dir(Direction::S2C, Flags::empty(), PacketType::Init);
		res.mac().copy_from_slice(b"TS3INIT1");
		res.packet_id(0x65);
		let content = res.data_mut();
		content.write_be(3u8).unwrap();
		content.write_all(x).unwrap();
		content.write_all(n).unwrap();
		content.write_be(level).unwrap();
		content.write_all(random2).unwrap();
		res
	}
}

impl OutAck {
	/// `for_type` is the packet type which gets acknowledged, so e.g. `Command`.
	pub fn new(dir: Direction, for_type: PacketType, packet_id: u16) -> OutPacket {
		let p_type = if for_type == PacketType::Command {
			PacketType::Ack
		} else if for_type == PacketType::CommandLow {
			PacketType::AckLow
		} else if for_type == PacketType::Ping {
			PacketType::Pong
		} else {
			panic!("Invalid packet type to create ack {:?}", for_type);
		};

		let mut res = OutPacket::new_with_dir(dir, Flags::empty(), p_type);
		let content = res.data_mut();
		content.write_be(packet_id).unwrap();
		res
	}
}

impl OutAudio {
	pub fn new(data: &AudioData) -> OutPacket {
		let mut res = OutPacket::new_with_dir(data.direction(), data.flags(), data.packet_type());
		let content = res.data_mut();

		content.write_be(data.id()).unwrap();
		match data {
			AudioData::C2S { codec, data, .. } => {
				content.write_be(codec.to_u8().unwrap()).unwrap();
				content.extend_from_slice(data);
			}
			AudioData::C2SWhisper { codec, channels, clients, data, .. } => {
				content.write_be(codec.to_u8().unwrap()).unwrap();
				content.write_be(channels.len() as u8).unwrap();
				content.write_be(clients.len() as u8).unwrap();

				for c in channels {
					content.write_be(*c).unwrap();
				}
				for c in clients {
					content.write_be(*c).unwrap();
				}
				content.extend_from_slice(data);
			}
			AudioData::C2SWhisperNew { codec, whisper_type, target, target_id, data, .. } => {
				content.write_be(codec.to_u8().unwrap()).unwrap();
				content.write_be(whisper_type.to_u8().unwrap()).unwrap();
				content.write_be(*target).unwrap();
				content.write_be(*target_id).unwrap();
				content.extend_from_slice(data);
			}
			AudioData::S2C { from, codec, data, .. }
			| AudioData::S2CWhisper { from, codec, data, .. } => {
				content.write_be(*from).unwrap();
				content.write_be(codec.to_u8().unwrap()).unwrap();
				content.extend_from_slice(data);
			}
		}

		res
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn test_audio_roundtrip(dir: Direction, data: &AudioData) {
		let out_p = OutAudio::new(data);
		let in_p = InPacket::try_new(dir, out_p.data()).unwrap();
		let audio = in_p.into_audio().unwrap();
		assert_eq!(audio.data(), data);
	}

	#[test]
	fn test_audio_c2s() {
		let data = AudioData::C2S { codec: CodecType::OpusVoice, id: 0x1234, data: &[1, 2, 3] };
		test_audio_roundtrip(Direction::C2S, &data);
	}

	#[test]
	fn test_audio_c2s_whisper() {
		let data = AudioData::C2SWhisper {
			codec: CodecType::OpusVoice,
			channels: vec![4, 5, 6],
			clients: vec![7, 8],
			id: 0x1234,
			data: &[1, 2, 3],
		};
		test_audio_roundtrip(Direction::C2S, &data);
	}

	#[test]
	fn test_audio_c2s_whisper_new() {
		let data = AudioData::C2SWhisperNew {
			codec: CodecType::OpusVoice,
			whisper_type: 3,
			target: 4,
			target_id: 5,
			id: 0x1234,
			data: &[1, 2, 3],
		};
		test_audio_roundtrip(Direction::C2S, &data);
	}

	#[test]
	fn test_audio_s2c() {
		let data = AudioData::S2C {
			codec: CodecType::OpusVoice,
			id: 0x1234,
			from: 0x5678,
			data: &[1, 2, 3],
		};
		test_audio_roundtrip(Direction::S2C, &data);
	}

	#[test]
	fn test_audio_s2c_whisper() {
		let data = AudioData::S2CWhisper {
			codec: CodecType::OpusVoice,
			id: 0x1234,
			from: 0x5678,
			data: &[1, 2, 3],
		};
		test_audio_roundtrip(Direction::S2C, &data);
	}
}
