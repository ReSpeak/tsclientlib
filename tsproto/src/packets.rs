#![allow(unused_variables)]
use std::fmt;
use std::io::prelude::*;
use std::io::Cursor;

use byteorder::{NetworkEndian, ReadBytesExt, WriteBytesExt};
use num::{FromPrimitive, ToPrimitive};

use {Error, Result};
use commands::Command;
use utils::HexSlice;

include!(concat!(env!("OUT_DIR"), "/packets.rs"));

#[derive(Debug, PartialEq, Eq, Clone, Copy, FromPrimitive, ToPrimitive, Hash)]
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

impl PacketType {
    pub fn is_command(&self) -> bool {
        *self == PacketType::Command || *self == PacketType::CommandLow
    }
    pub fn is_voice(&self) -> bool {
        *self == PacketType::Voice || *self == PacketType::VoiceWhisper
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

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct UdpPacket(pub Vec<u8>);

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
