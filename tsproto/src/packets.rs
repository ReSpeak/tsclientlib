#![allow(unused_variables)]
use std::borrow::Cow;
use std::collections::HashMap;
use std::io::prelude::*;
use std::io::Cursor;
use std::{fmt, mem};

use byteorder::{NetworkEndian, ReadBytesExt, WriteBytesExt};
use bytes::Bytes;
use num_traits::{FromPrimitive, ToPrimitive};

use crate::commands::Command;
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

#[derive(Debug, Clone)]
pub struct CommandData<'a> {
	/// The name is empty for serverquery commands
	pub name: &'a str,
	pub static_args: Vec<(&'a str, Cow<'a, str>)>,
	pub list_args: Vec<Vec<(&'a str, Cow<'a, str>)>>,
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
		/// Has to be a `clientinitiv alpha=… beta=…` command.
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
	pub fn direction(&self) -> Direction { self.dir }
	#[inline]
	pub fn name(&self) -> &str { self.inner.ref_rent(|d| d.name) }
	#[inline]
	pub fn with_data<R, F: FnOnce(&CommandData) -> R>(&self, f: F) -> R {
		self.inner.rent(f)
	}

	pub fn iter(&self) -> InCommandIterator {
		let statics = self
			.inner
			.suffix()
			.static_args
			.iter()
			.map(|(a, b)| (*a, b.as_ref()))
			.collect();
		InCommandIterator {
			cmd: self,
			statics,
			i: 0,
		}
	}
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalCommand<'a>(pub HashMap<&'a str, &'a str>);
impl<'a> CanonicalCommand<'a> {
	pub fn has_arg(&self, arg: &str) -> bool { self.0.contains_key(arg) }
}

pub struct InCommandIterator<'a> {
	cmd: &'a InCommand,
	statics: HashMap<&'a str, &'a str>,
	i: usize,
}

impl<'a> Iterator for InCommandIterator<'a> {
	type Item = CanonicalCommand<'a>;
	fn next(&mut self) -> Option<Self::Item> {
		let i = self.i;
		self.i += 1;
		let c = self.cmd.inner.suffix();
		if c.list_args.is_empty() {
			if i == 0 {
				Some(CanonicalCommand(mem::replace(
					&mut self.statics,
					HashMap::new(),
				)))
			} else {
				None
			}
		} else if i < c.list_args.len() {
			let l = &c.list_args[i];
			let mut v = self.statics.clone();
			v.extend(l.iter().map(|(k, v)| (*k, v.as_ref())));
			Some(CanonicalCommand(v))
		} else {
			None
		}
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
