#![allow(unused_variables)]
use std::borrow::Cow;
use std::io::prelude::*;
use std::io::Cursor;
use std::{fmt, mem};

use arrayref::{array_mut_ref, array_ref};
use bitflags::bitflags;
use byteorder::{NetworkEndian, ReadBytesExt, WriteBytesExt};
use bytes::Bytes;
use num_traits::{FromPrimitive, ToPrimitive};

use crate::commands::{CommandData, CommandDataIterator, Command};
use crate::utils::HexSlice;
use crate::{Error, Result};

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
	pub fn is_ack(self) -> bool {
		self == PacketType::Ack || self == PacketType::AckLow
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

/// Used for debugging.
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
	#[rustfmt::skip]
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "UdpPacket {{ header: ")?;

		// Parse header
		if let Ok(header) =
			Header::read(&self.from_client, &mut Cursor::new(self.data))
		{
			write!(f, "mac: {:?}, ", header.mac)?;
			write!(f, "p_id: {}, ", header.p_id)?;
			if let Some(c_id) = header.c_id {
				write!(f, "c_id: {}, ", c_id)?;
			}
			write!(f, "type: {:?}, ", PacketType::from_u8(header.p_type & 0xf).unwrap())?;
			write!(f, "flags: ")?;
			let flags = Flags::from_bits(header.p_type & 0xf0).unwrap();
			write!(f, "{}", if flags.contains(Flags::UNENCRYPTED) { "u" } else { "-" })?;
			write!(f, "{}", if flags.contains(Flags::COMPRESSED) { "c" } else { "-" })?;
			write!(f, "{}", if flags.contains(Flags::NEWPROTOCOL) { "n" } else { "-" })?;
			write!(f, "{}", if flags.contains(Flags::FRAGMENTED) { "f" } else { "-" })?;
		}

		write!(f, ", raw: {:?} }}", HexSlice(self.data))?;
		Ok(())
	}
}

/// Used for debugging.
pub struct InUdpPacket<'a>(&'a InPacket);

impl<'a> InUdpPacket<'a> {
	pub fn new(packet: &'a InPacket) -> Self { Self(packet) }
}

impl<'a> fmt::Debug for InUdpPacket<'a> {
	#[rustfmt::skip]
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "UdpPacket {{ header: {{ ")?;

		let header = self.0.header();
		write!(f, "mac: {:?}, ", header.mac())?;
		write!(f, "p_id: {}, ", header.packet_id())?;
		if let Some(c_id) = header.client_id() {
			write!(f, "c_id: {}, ", c_id)?;
		}
		write!(f, "type: {:?}, ", header.packet_type())?;
		write!(f, "flags: ")?;
		let flags = header.flags();
		write!(f, "{}", if flags.contains(Flags::UNENCRYPTED) { "u" } else { "-" })?;
		write!(f, "{}", if flags.contains(Flags::COMPRESSED) { "c" } else { "-" })?;
		write!(f, "{}", if flags.contains(Flags::NEWPROTOCOL) { "n" } else { "-" })?;
		write!(f, "{}", if flags.contains(Flags::FRAGMENTED) { "f" } else { "-" })?;

		write!(f, "}}, raw: {:?} }}", HexSlice(self.0.header_data()))?;
		Ok(())
	}
}

#[derive(Debug)]
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

		#[rental(covariant, debug)]
		pub(crate) struct Packet {
			header: MyBytes,
			content: Cow<'header, [u8]>,
		}

		#[rental(covariant, debug)]
		pub(crate) struct Command {
			content: Vec<u8>,
			data: CommandData<'content>,
		}

		#[rental(debug)]
		pub(crate) struct S2CInit {
			#[subrental = 2]
			packet: Box<Packet>,
			data: S2CInitData<'packet_1>,
		}

		#[rental(debug)]
		pub(crate) struct C2SInit {
			#[subrental = 2]
			packet: Box<Packet>,
			data: C2SInitData<'packet_1>,
		}

		#[rental(debug)]
		pub(crate) struct Audio {
			#[subrental = 2]
			packet: Box<Packet>,
			data: VoiceData<'packet_1>,
		}
	}
}

pub struct InPacket {
	inner: rentals::Packet,
	dir: Direction,
}

pub struct InHeader<'a>(&'a [u8], Direction);

#[derive(Debug)]
pub struct InCommand {
	inner: rentals::Command,
	p_type: PacketType,
	newprotocol: bool,
	dir: Direction,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct OutPacket {
	dir: Direction,
	data: Vec<u8>,
}

/// The mac has to be `b"TS3INIT1"`.
///
/// `version` always contains the Teamspeak version as timestamp.
///
/// `timestamp` contains a current timestamp.
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
		command: CommandData<'a>,
	},
}

pub enum S2CInitData<'a> {
	Init1 {
		random1: &'a [u8; 16],
		random0_r: &'a [u8; 4],
	},
	Init3 {
		x: &'a [u8; 64],
		n: &'a [u8; 64],
		level: u32,
		random2: &'a [u8; 100],
	},
}

#[derive(Debug)]
pub struct InS2CInit(rentals::S2CInit);
#[derive(Debug)]
pub struct InC2SInit(rentals::C2SInit);

#[derive(Debug)]
pub enum VoiceData<'a> {
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

#[derive(Debug)]
pub struct InAudio(rentals::Audio);

impl Direction {
	pub fn reverse(self) -> Self {
		match self {
			Direction::S2C => Direction::C2S,
			Direction::C2S => Direction::S2C,
		}
	}
}

impl InPacket {
	/// Do some sanity checks before creating the object.
	pub fn try_new(data: Bytes, dir: Direction) -> Result<Self> {
		let header_len = if dir == Direction::S2C {
			crate::S2C_HEADER_LEN
		} else {
			crate::C2S_HEADER_LEN
		};
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
			inner: rentals::Packet::new(MyBytes(data), |data| {
				if dir == Direction::S2C {
					Cow::Borrowed(&data[crate::S2C_HEADER_LEN..])
				} else {
					Cow::Borrowed(&data[crate::C2S_HEADER_LEN..])
				}
			}),
			dir,
		}
	}

	#[inline]
	fn header_data(&self) -> &[u8] { self.inner.head() }

	#[inline]
	pub fn set_content(&mut self, content: Vec<u8>) {
		self.inner.rent_mut(|c| *c = Cow::Owned(content));
	}

	#[inline]
	pub fn content(&self) -> &[u8] { self.inner.ref_rent(|d| &**d) }

	#[inline]
	pub fn take_content(&mut self) -> Vec<u8> {
		self.inner
			.rent_mut(|d| mem::replace(d, Cow::Borrowed(&[])).into_owned())
	}

	#[inline]
	pub fn header(&self) -> InHeader { InHeader(self.inner.head(), self.dir) }
	#[inline]
	pub fn direction(&self) -> Direction { self.dir }

	/// Get the acknowledged packet id if this is an ack packet.
	#[inline]
	pub fn ack_packet(&self) -> Option<u16> {
		let p_type = self.header().packet_type();
		if p_type.is_ack() {
			self.content().read_u16::<NetworkEndian>().ok()
		} else {
			None
		}
	}

	/// Parse this packet into a voice packet.
	pub fn into_audio(self) -> Result<InAudio> {
		let p_type = self.header().packet_type();
		let newprotocol = self.header().flags().contains(Flags::NEWPROTOCOL);
		let dir = self.dir;
		let id = self.content().read_u16::<NetworkEndian>()?;

		Ok(InAudio(rentals::Audio::try_new_or_drop(
			Box::new(self.inner),
			|p| -> Result<_> {
				let content = p.content;
				if content.len() < 5 {
					return Err(format_err!("Voice packet too short").into());
				}
				if p_type == PacketType::Voice {
					if dir == Direction::S2C {
						Ok(VoiceData::S2C {
							id,
							from: (&content[2..])
								.read_u16::<NetworkEndian>()?,
							codec: CodecType::from_u8(content[4])
								.ok_or_else::<Error, _>(|| {
									format_err!("Invalid codec").into()
								})?,
							data: &content[5..],
						})
					} else {
						Ok(VoiceData::C2S {
							id,
							codec: CodecType::from_u8(content[3])
								.ok_or_else::<Error, _>(|| {
									format_err!("Invalid codec").into()
								})?,
							data: &content[4..],
						})
					}
				} else if dir == Direction::S2C {
					Ok(VoiceData::S2CWhisper {
						id,
						from: (&content[2..]).read_u16::<NetworkEndian>()?,
						codec: CodecType::from_u8(content[4])
							.ok_or_else::<Error, _>(|| {
								format_err!("Invalid codec").into()
							})?,
						data: &content[5..],
					})
				} else {
					let codec = CodecType::from_u8(content[3])
						.ok_or_else::<Error, _>(|| {
							format_err!("Invalid codec").into()
						})?;
					if newprotocol {
						if content.len() < 13 {
							return Err(
								format_err!("Voice packet too short").into()
							);
						}
						Ok(VoiceData::C2SWhisperNew {
							id,
							codec,
							whisper_type: content[4],
							target: content[5],
							target_id: (&content[6..])
								.read_u64::<NetworkEndian>()?,
							data: &content[13..],
						})
					} else {
						if content.len() < 5 {
							return Err(
								format_err!("Voice packet too short").into()
							);
						}
						let channel_count = content[4] as usize;
						let client_count = content[5] as usize;
						let channel_off = 5;
						let client_off = channel_off + channel_count * 8;
						let off = client_off + client_count * 2;
						if content.len() < off {
							return Err(
								format_err!("Voice packet too short").into()
							);
						}

						Ok(VoiceData::C2SWhisper {
							id,
							codec,
							channels: (0..channel_count)
								.map(|i| {
									(&content[channel_off + i * 8..])
										.read_u64::<NetworkEndian>()
								})
								.collect::<::std::result::Result<Vec<_>, _>>(
								)?,
							clients: (0..client_count)
								.map(|i| {
									(&content[client_off + i * 2..])
										.read_u16::<NetworkEndian>()
								})
								.collect::<::std::result::Result<Vec<_>, _>>(
								)?,
							data: &content[off..],
						})
					}
				}
			},
		)?))
	}

	pub fn into_s2cinit(self) -> Result<InS2CInit> {
		if self.dir != Direction::S2C {
			return Err(format_err!("Wrong direction").into());
		}
		if self.header().packet_type() != PacketType::Init {
			return Err(format_err!("Not an init packet").into());
		}
		if self.header().mac() != b"TS3INIT1" {
			return Err(format_err!("Wrong init packet mac").into());
		}

		Ok(InS2CInit(rentals::S2CInit::try_new_or_drop(
			Box::new(self.inner),
			|p| -> Result<_> {
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
			},
		)?))
	}

	pub fn into_c2sinit(self) -> Result<InC2SInit> {
		if self.dir != Direction::C2S {
			return Err(format_err!("Wrong direction").into());
		}
		if self.header().packet_type() != PacketType::Init {
			return Err(format_err!("Not an init packet").into());
		}
		if self.header().mac() != b"TS3INIT1" {
			return Err(format_err!("Wrong init packet mac").into());
		}

		Ok(InC2SInit(rentals::C2SInit::try_new_or_drop(
			Box::new(self.inner),
			|p| -> Result<_> {
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
						timestamp: (&content[5..])
							.read_u32::<NetworkEndian>()?,
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
					let command = crate::commands::parse_command2(s)?;
					Ok(C2SInitData::Init4 {
						version,
						x: array_ref!(content, 5, 64),
						n: array_ref!(content, 69, 64),
						level: (&content[128 + 5..])
							.read_u32::<NetworkEndian>()?,
						random2: array_ref!(content, 128 + 9, 100),
						y: array_ref!(content, 228 + 9, 64),
						command,
					})
				} else {
					Err(format_err!("Invalid init step").into())
				}
			},
		)?))
	}
}

impl fmt::Debug for InPacket {
	#[rustfmt::skip]
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "Packet {{ header: {{ ")?;

		let header = self.header();
		write!(f, "mac: {:?}, ", header.mac())?;
		write!(f, "p_id: {}, ", header.packet_id())?;
		if let Some(c_id) = header.client_id() {
			write!(f, "c_id: {}, ", c_id)?;
		}
		write!(f, "type: {:?}, ", header.packet_type())?;
		write!(f, "flags: ")?;
		let flags = header.flags();
		write!(f, "{}", if flags.contains(Flags::UNENCRYPTED) { "u" } else { "-" })?;
		write!(f, "{}", if flags.contains(Flags::COMPRESSED) { "c" } else { "-" })?;
		write!(f, "{}", if flags.contains(Flags::NEWPROTOCOL) { "n" } else { "-" })?;
		write!(f, "{}", if flags.contains(Flags::FRAGMENTED) { "f" } else { "-" })?;

		write!(f, "}}, content: {:?} }}", HexSlice(self.content()))?;
		Ok(())
	}
}

impl<'a> InHeader<'a> {
	/// The offset to the packet type.
	#[inline]
	fn get_off(&self) -> usize {
		if self.1 == Direction::S2C {
			10
		} else {
			12
		}
	}

	#[inline]
	pub fn mac(&self) -> &[u8; 8] { array_ref![self.0, 0, 8] }

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
		Flags::from_bits(self.0[self.get_off()] & 0xf0).unwrap()
	}

	#[inline]
	pub fn packet_type(&self) -> PacketType {
		PacketType::from_u8(self.0[self.get_off()] & 0xf).unwrap()
	}

	pub fn get_meta(&self) -> Vec<u8> {
		let mut res = Vec::with_capacity(5);
		res.write_u16::<NetworkEndian>(self.packet_id()).unwrap();
		if let Some(c_id) = self.client_id() {
			res.write_u16::<NetworkEndian>(c_id).unwrap();
		}
		res.write_u8(self.0[self.get_off()]).unwrap();
		res
	}
}

impl<'a> fmt::Debug for C2SInitData<'a> {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		match self {
			C2SInitData::Init0 { .. } => write!(f, "C2SInitData::Init0"),
			C2SInitData::Init2 { .. } => write!(f, "C2SInitData::Init2"),
			C2SInitData::Init4 { .. } => write!(f, "C2SInitData::Init4"),
		}
	}
}

impl<'a> fmt::Debug for S2CInitData<'a> {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		match self {
			S2CInitData::Init1 { .. } => write!(f, "S2CInitData::Init1"),
			S2CInitData::Init3 { .. } => write!(f, "S2CInitData::Init3"),
		}
	}
}

impl InS2CInit {
	#[inline]
	pub fn with_data<R, F: FnOnce(&S2CInitData) -> R>(&self, f: F) -> R {
		self.0.rent(f)
	}
}

impl InCommand {
	pub fn new(
		content: Vec<u8>,
		p_type: PacketType,
		newprotocol: bool,
		dir: Direction,
	) -> Result<Self>
	{
		let inner = rentals::Command::try_new_or_drop(content, |c| {
			let s = ::std::str::from_utf8(c)?;
			crate::commands::parse_command2(s)
		})?;
		Ok(Self {
			inner,
			p_type,
			newprotocol,
			dir,
		})
	}

	pub fn with_content(packet: &InPacket, content: Vec<u8>) -> Result<Self> {
		let header = packet.header();
		Self::new(
			content,
			header.packet_type(),
			header.flags().contains(Flags::NEWPROTOCOL),
			packet.dir,
		)
	}

	#[inline]
	pub fn packet_type(&self) -> PacketType { self.p_type }
	#[inline]
	pub fn newprotocol(&self) -> bool { self.newprotocol }
	#[inline]
	pub fn direction(&self) -> Direction { self.dir }
	#[inline]
	pub fn name(&self) -> &str { self.inner.ref_rent(|d| d.name) }
	#[inline]
	pub fn data(&self) -> &CommandData { self.inner.suffix() }

	#[inline]
	pub fn iter(&self) -> CommandDataIterator { self.inner.suffix().iter() }
}

impl OutPacket {
	#[inline]
	pub fn new(mac: [u8; 8], packet_id: u16, client_id: Option<u16>,
		   flags: Flags, packet_type: PacketType) -> Self {
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
		let data = vec![0; if dir == Direction::S2C { crate::S2C_HEADER_LEN }
			else { crate::C2S_HEADER_LEN }];
		let mut res = Self { dir, data };
		res.flags(flags);
		res.packet_type(packet_type);
		res
	}

	#[inline]
	pub fn new_from_data(dir: Direction, data: Vec<u8>) -> Self {
		Self { dir, data }
	}

	#[inline]
	pub fn into_vec(self) -> Vec<u8> { self.data }

	#[inline]
	fn content_offset(&self) -> usize {
		if self.dir == Direction::S2C {
			crate::S2C_HEADER_LEN
		} else {
			crate::C2S_HEADER_LEN
		}
	}

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
	pub fn header(&self) -> InHeader { InHeader(&self.data, self.dir) }
	#[inline]
	pub fn header_bytes(&self) -> &[u8] {
		&self.data[..self.content_offset()]
	}

	#[inline]
	pub fn mac(&mut self) -> &mut [u8; 8] { array_mut_ref!(self.data, 0, 8) }
	#[inline]
	pub fn packet_id(&mut self, packet_id: u16) {
		(&mut self.data[8..10]).write_u16::<NetworkEndian>(packet_id).unwrap();
	}
	#[inline]
	pub fn client_id(&mut self, client_id: u16) {
		// Client id is only valid for client to server packets.
		assert_eq!(self.dir, Direction::C2S);
		(&mut self.data[10..12]).write_u16::<NetworkEndian>(client_id).unwrap();
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

pub struct OutCommand;
impl OutCommand {
	/// Write a command.
	///
	/// # Examples
	/// Write a command from existing `CommandData`.
	/// ```
	/// let command = crate::commands::parse_command2("").unwrap();
	/// tsproto::packets::OutCommand::new(command.name,
	///     command.static_args.iter().map(|(k, v)| (*k, v.as_ref())),
	///     command.list_args.iter().map(|i| {
	///         i.iter().map(|(k, v)| (*k, v.as_ref()))
	///     }),
	/// )
	/// ```
	pub fn new<K1, V1, K2, V2, I1, I2, I3>(
		dir: Direction,
		p_type: PacketType,
		name: &str,
		static_args: I1,
		list_args: I2,
	) -> OutPacket
		where
			K1: AsRef<str>,
			V1: AsRef<str>,
			K2: AsRef<str>,
			V2: AsRef<str>,
			I1: Iterator<Item=(K1, V1)>,
			I2: Iterator<Item=I3>,
			I3: Iterator<Item=(K2, V2)>,
	{
		let mut res = OutPacket::new_with_dir(dir, Flags::empty(), p_type);
		let content = res.data_mut();
		Self::new_into(name, static_args, list_args, content);
		res
	}

	/// Write a command into a given `Vec<u8>`.
	pub fn new_into<K1, V1, K2, V2, I1, I2, I3>(
		name: &str,
		static_args: I1,
		list_args: I2,
		res: &mut Vec<u8>,
	)
		where
			K1: AsRef<str>,
			V1: AsRef<str>,
			K2: AsRef<str>,
			V2: AsRef<str>,
			I1: Iterator<Item=(K1, V1)>,
			I2: Iterator<Item=I3>,
			I3: Iterator<Item=(K2, V2)>,
	{
		res.extend_from_slice(name.as_bytes());
		for (k, v) in static_args {
			let k = k.as_ref();
			let v = v.as_ref();
			if !name.is_empty() {
				res.push(b' ');
			}
			res.extend_from_slice(k.as_bytes());
			if !v.is_empty() {
				res.push(b'=');
			}
			Self::write_escaped(res, v).unwrap();
		}

		let mut first_list = true;
		for i in list_args {
			if !name.is_empty() {
				res.push(b' ');
			}
			if first_list {
				first_list = false;
			} else {
				res.push(b'|');
			}

			let mut first = true;
			for (k, v) in i {
				if first {
					first = false;
				} else {
					res.push(b' ');
				}
				let k = k.as_ref();
				let v = v.as_ref();
				res.extend_from_slice(k.as_bytes());
				if !v.is_empty() {
					res.push(b'=');
				}
				Self::write_escaped(res, v).unwrap();
			}
		}
	}

	fn write_escaped(w: &mut Write, s: &str) -> Result<()> {
		for c in s.chars() {
			match c {
				'\u{b}' => write!(w, "\\v"),
				'\u{c}' => write!(w, "\\f"),
				'\\' => write!(w, "\\\\"),
				'\t' => write!(w, "\\t"),
				'\r' => write!(w, "\\r"),
				'\n' => writeln!(w),
				'|' => write!(w, "\\p"),
				' ' => write!(w, "\\s"),
				'/' => write!(w, "\\/"),
				c => write!(w, "{}", c),
			}?;
		}
		Ok(())
	}
}

pub struct OutC2SInit0;
impl OutC2SInit0 {
	pub fn new(version: u32, timestamp: u32, random0: &[u8; 4]) -> OutPacket {
		let mut res = OutPacket::new_with_dir(Direction::C2S, Flags::empty(), PacketType::Init);
		res.mac().copy_from_slice(b"TS3INIT1");
		res.packet_id(0x65);
		let content = res.data_mut();
		content.write_u32::<NetworkEndian>(version).unwrap();
		content.write_u8(0).unwrap();
		content.write_u32::<NetworkEndian>(timestamp).unwrap();
		content.write_all(random0).unwrap();
		// Reserved
		content.write_all(&[0u8; 8]).unwrap();
		res
	}
}

pub struct OutC2SInit2;
impl OutC2SInit2 {
	pub fn new(version: u32, random1: &[u8; 16], random0_r: &[u8; 4]) -> OutPacket {
		let mut res = OutPacket::new_with_dir(Direction::C2S, Flags::empty(), PacketType::Init);
		res.mac().copy_from_slice(b"TS3INIT1");
		res.packet_id(0x65);
		let content = res.data_mut();
		content.write_u32::<NetworkEndian>(version).unwrap();
		content.write_u8(2).unwrap();
		content.write_all(random1).unwrap();
		content.write_all(random0_r).unwrap();
		res
	}
}

pub struct OutC2SInit4;
impl OutC2SInit4 {
	pub fn new(version: u32, x: &[u8; 64], n: &[u8; 64], level: u32,
		random2: &[u8; 100], y: &[u8; 64], alpha: &[u8], omega: &[u8], ip: &str) -> OutPacket {
		let mut res = OutPacket::new_with_dir(Direction::C2S, Flags::empty(), PacketType::Init);
		res.mac().copy_from_slice(b"TS3INIT1");
		res.packet_id(0x65);
		let content = res.data_mut();
		content.write_u32::<NetworkEndian>(version).unwrap();
		content.write_u8(4).unwrap();
		content.write_all(x).unwrap();
		content.write_all(n).unwrap();
		content.write_u32::<NetworkEndian>(level).unwrap();
		content.write_all(random2).unwrap();
		content.write_all(y).unwrap();
		let ip = if ip.is_empty() {
			String::new()
		} else {
			format!("={}", ip)
		};
		content.write_all(format!("clientinitiv alpha={} omega={} ot=1 ip{}",
			base64::encode(alpha), base64::encode(omega), ip).as_bytes()).unwrap();
		res
	}
}

pub struct OutS2CInit1;
impl OutS2CInit1 {
	pub fn new(random1: &[u8; 16], random0_r: &[u8; 4]) -> OutPacket {
		let mut res = OutPacket::new_with_dir(Direction::C2S, Flags::empty(), PacketType::Init);
		res.mac().copy_from_slice(b"TS3INIT1");
		res.packet_id(0x65);
		let content = res.data_mut();
		content.write_u8(1).unwrap();
		content.write_all(random1).unwrap();
		content.write_all(random0_r).unwrap();
		res
	}
}

pub struct OutS2CInit3;
impl OutS2CInit3 {
	pub fn new(x: &[u8; 64], n: &[u8; 64], level: u32, random2: &[u8; 100]) -> OutPacket {
		let mut res = OutPacket::new_with_dir(Direction::C2S, Flags::empty(), PacketType::Init);
		res.mac().copy_from_slice(b"TS3INIT1");
		res.packet_id(0x65);
		let content = res.data_mut();
		content.write_u8(3).unwrap();
		content.write_all(x).unwrap();
		content.write_all(n).unwrap();
		content.write_u32::<NetworkEndian>(level).unwrap();
		content.write_all(random2).unwrap();
		res
	}
}

pub struct OutAck;
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
		content.write_u16::<NetworkEndian>(packet_id).unwrap();
		res
	}
}

impl Packet {
	pub fn new(header: Header, data: Data) -> Packet { Packet { header, data } }
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
	#[inline]
	pub fn new(p_type: PacketType) -> Header {
		let mut h = Header::default();
		h.set_type(p_type);
		h
	}

	#[inline]
	pub fn get_p_type(&self) -> u8 { self.p_type }
	#[inline]
	pub fn set_p_type(&mut self, p_type: u8) {
		assert!((p_type & 0xf) <= 8);
		self.p_type = p_type;
	}

	/// `true` if the packet is not encrypted.
	#[inline]
	pub fn get_unencrypted(&self) -> bool { (self.get_p_type() & 0x80) != 0 }
	/// `true` if the packet is compressed.
	#[inline]
	pub fn get_compressed(&self) -> bool { (self.get_p_type() & 0x40) != 0 }
	#[inline]
	pub fn get_newprotocol(&self) -> bool { (self.get_p_type() & 0x20) != 0 }
	/// `true` for the first and last packet of a compressed series of packets.
	#[inline]
	pub fn get_fragmented(&self) -> bool { (self.get_p_type() & 0x10) != 0 }

	#[inline]
	pub fn set_unencrypted(&mut self, value: bool) {
		let p_type = self.get_p_type();
		if value {
			self.set_p_type(p_type | 0x80);
		} else {
			self.set_p_type(p_type & !0x80);
		}
	}
	#[inline]
	pub fn set_compressed(&mut self, value: bool) {
		let p_type = self.get_p_type();
		if value {
			self.set_p_type(p_type | 0x40);
		} else {
			self.set_p_type(p_type & !0x40);
		}
	}
	#[inline]
	pub fn set_newprotocol(&mut self, value: bool) {
		let p_type = self.get_p_type();
		if value {
			self.set_p_type(p_type | 0x20);
		} else {
			self.set_p_type(p_type & !0x20);
		}
	}
	#[inline]
	pub fn set_fragmented(&mut self, value: bool) {
		let p_type = self.get_p_type();
		if value {
			self.set_p_type(p_type | 0x10);
		} else {
			self.set_p_type(p_type & !0x10);
		}
	}

	#[inline]
	pub fn get_type(&self) -> PacketType {
		PacketType::from_u8(self.get_p_type() & 0xf).unwrap()
	}
	#[inline]
	pub fn set_type(&mut self, t: PacketType) {
		let p_type = self.get_p_type();
		self.set_p_type((p_type & 0xf0) | t.to_u8().unwrap());
	}

	#[inline]
	pub fn write_meta(&self, w: &mut Write) -> Result<()> {
		w.write_u16::<NetworkEndian>(self.p_id)?;
		if let Some(c_id) = self.c_id {
			w.write_u16::<NetworkEndian>(c_id)?;
		}
		w.write_u8(self.p_type)?;
		Ok(())
	}
}
