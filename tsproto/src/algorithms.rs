//! Handle packet splitting and cryptography
use curve25519_dalek_ng::edwards::EdwardsPoint;
use eax::aead::consts::{U16, U8};
use eax::{AeadInPlace, Eax, KeyInit};
use generic_array::GenericArray;
use num_bigint::BigUint;
use num_traits::ToPrimitive;
use omnom::WriteExt;
use quicklz::CompressionLevel;
use sha1::Sha1;
use sha2::{Digest, Sha256, Sha512};
use tsproto_types::crypto::{EccKeyPrivEd25519, EccKeyPubP256};

use crate::connection::CachedKey;
use crate::{Error, Result};
use tsproto_packets::packets::*;

pub fn must_encrypt(t: PacketType) -> bool {
	match t {
		PacketType::Command | PacketType::CommandLow => true,
		PacketType::Voice
		| PacketType::Ack
		| PacketType::AckLow
		| PacketType::VoiceWhisper
		| PacketType::Ping
		| PacketType::Pong
		| PacketType::Init => false,
	}
}

pub fn should_encrypt(t: PacketType, voice_encryption: bool) -> bool {
	must_encrypt(t)
		|| t == PacketType::Ack
		|| t == PacketType::AckLow
		|| (voice_encryption && t.is_voice())
}

/// Compresses and splits the packet data of a `Command` or `CommandLow` packet.
///
/// Returns the splitted packet data and their headers.
/// The headers have their type and the compressed and fragmented flag set
/// to the right value.
///
/// Returns an error if the packet is too large but cannot be splitted.
/// Only `Command` and `CommandLow` packets can be compressed and splitted.
pub fn compress_and_split(is_client: bool, packet: OutPacket) -> Vec<OutPacket> {
	// Everything except whisper packets has to be less than 500 bytes
	let header_size =
		if is_client { tsproto_packets::C2S_HEADER_LEN } else { tsproto_packets::S2C_HEADER_LEN };
	let data = packet.content();
	// The maximum packet size (including header) is 500 bytes.
	let max_size = 500 - header_size;
	// Split the data if it is necessary.
	let compressed;
	let datas = if data.len() > max_size {
		// Compress with QuickLZ
		let cdata = ::quicklz::compress(data, CompressionLevel::Lvl1);
		// Use only if it is efficient
		let mut data = if cdata.len() > data.len() {
			compressed = false;
			data
		} else {
			compressed = true;
			&cdata
		};

		// Ignore size limit for whisper packets
		if data.len() <= max_size || packet.header().packet_type() == PacketType::VoiceWhisper {
			let mut v = vec![0; header_size + data.len()];
			v[header_size..].copy_from_slice(data);
			vec![v]
		} else {
			// Split
			let count = (data.len() + max_size - 1) / max_size;
			let mut splitted = Vec::with_capacity(count);
			while data.len() > max_size {
				let (first, last) = data.split_at(max_size);
				let mut v = vec![0; header_size + max_size];
				v[header_size..].copy_from_slice(first);
				splitted.push(v);
				data = last;
			}
			// Rest
			let mut v = vec![0; header_size + data.len()];
			v[header_size..].copy_from_slice(data);
			splitted.push(v);
			splitted
		}
	} else {
		return vec![packet];
	};

	let len = datas.len();
	let fragmented = len > 1;
	let orig_header = packet.header_bytes();
	let dir = packet.direction();
	let mut packets = Vec::with_capacity(datas.len());
	for (i, mut d) in datas.into_iter().enumerate() {
		d[..header_size].copy_from_slice(orig_header);
		let mut packet = OutPacket::new_from_data(dir, d);
		// Only set flags on first fragment
		if i == 0 && compressed {
			packet.flags(packet.header().flags() | Flags::COMPRESSED);
		}

		// Set fragmented flag on first and last part
		if fragmented && (i == 0 || i == len - 1) {
			packet.flags(packet.header().flags() | Flags::FRAGMENTED);
		}
		packets.push(packet);
	}
	packets
}

fn create_key_nonce(
	p_type: PacketType, c_id: Option<u16>, p_id: u16, generation_id: u32, iv: &[u8; 64],
	cache: &mut [[CachedKey; 2]; 8],
) -> (GenericArray<u8, U16>, GenericArray<u8, U16>) {
	// Check if this generation is cached
	let cache = &mut cache[p_type.to_usize().unwrap()][if c_id.is_some() { 1 } else { 0 }];
	if cache.generation_id != generation_id {
		// Update the cache
		let mut temp = [0; 70];
		if c_id.is_some() {
			temp[0] = 0x31;
		} else {
			temp[0] = 0x30;
		}
		temp[1] = p_type.to_u8().unwrap();
		(&mut temp[2..6]).write_be(generation_id).unwrap();
		temp[6..].copy_from_slice(iv);

		let keynonce = Sha256::digest(&temp[..]);
		let keynonce = keynonce.as_slice();
		cache.generation_id = generation_id;
		cache.key.copy_from_slice(&keynonce[..16]);
		cache.nonce.copy_from_slice(&keynonce[16..]);
	}

	// Use the cached version
	let mut key = cache.key;
	let nonce = cache.nonce;
	key[0] ^= (p_id >> 8) as u8;
	key[1] ^= (p_id & 0xff) as u8;
	(key, nonce)
}

pub fn encrypt_key_nonce(
	packet: &mut OutPacket, key: &GenericArray<u8, U16>, nonce: &GenericArray<u8, U16>,
) -> Result<()> {
	let meta = packet.header().get_meta().to_vec();
	let cipher = Eax::<aes::Aes128, U8>::new(key);
	let mac = cipher
		.encrypt_in_place_detached(nonce, &meta, packet.content_mut())
		.map_err(|_| Error::MaxLengthExceeded("encryption data"))?;
	packet.mac().copy_from_slice(mac.as_slice());
	Ok(())
}

pub fn encrypt_fake(packet: &mut OutPacket) -> Result<()> {
	encrypt_key_nonce(packet, &crate::FAKE_KEY.into(), &crate::FAKE_NONCE.into())
}

pub fn encrypt(
	packet: &mut OutPacket, generation_id: u32, iv: &[u8; 64], cache: &mut [[CachedKey; 2]; 8],
) -> Result<()> {
	let header = packet.header();
	let (key, nonce) = create_key_nonce(
		header.packet_type(),
		header.client_id(),
		header.packet_id(),
		generation_id,
		iv,
		cache,
	);
	encrypt_key_nonce(packet, &key, &nonce)
}

pub fn decrypt_key_nonce(
	packet: &InPacket, key: &GenericArray<u8, U16>, nonce: &GenericArray<u8, U16>,
) -> Result<Vec<u8>> {
	let header = packet.header();
	let meta = header.get_meta();
	// TODO decrypt in-place in packet
	let mut content = packet.content().to_vec();
	let cipher = Eax::<aes::Aes128, U8>::new(key);
	cipher
		.decrypt_in_place_detached(nonce, meta, &mut content, header.mac().into())
		.map(|()| content)
		.map_err(|_| Error::WrongMac {
			p_type: header.packet_type(),
			generation_id: 0,
			packet_id: header.packet_id(),
		})
}

pub fn decrypt_fake(packet: &InPacket) -> Result<Vec<u8>> {
	decrypt_key_nonce(packet, &crate::FAKE_KEY.into(), &crate::FAKE_NONCE.into())
}

pub fn decrypt(
	packet: &InPacket, generation_id: u32, iv: &[u8; 64], cache: &mut [[CachedKey; 2]; 8],
) -> Result<Vec<u8>> {
	let header = packet.header();
	let (key, nonce) = create_key_nonce(
		header.packet_type(),
		header.client_id(),
		header.packet_id(),
		generation_id,
		iv,
		cache,
	);
	decrypt_key_nonce(packet, &key, &nonce).map_err(|e| {
		if let Error::WrongMac { p_type, packet_id, .. } = e {
			Error::WrongMac { p_type, generation_id, packet_id }
		} else {
			e
		}
	})
}

/// Compute shared iv and shared mac.
pub fn compute_iv_mac(
	alpha: &[u8; 10], beta: &[u8; 54], our_key: &EccKeyPrivEd25519, other_key: &EdwardsPoint,
) -> ([u8; 64], [u8; 8]) {
	let shared_secret = our_key.create_shared_secret(other_key);
	let mut shared_iv = [0; 64];
	shared_iv.copy_from_slice(Sha512::digest(shared_secret).as_slice());
	for i in 0..10 {
		shared_iv[i] ^= alpha[i];
	}
	for i in 0..54 {
		shared_iv[i + 10] ^= beta[i];
	}
	let mut shared_mac = [0; 8];
	shared_mac.copy_from_slice(&Sha1::digest(shared_iv).as_slice()[..8]);
	(shared_iv, shared_mac)
}

pub fn hash_cash(key: &EccKeyPubP256, level: u8) -> u64 {
	let omega = key.to_ts();
	let mut offset = 0;
	while offset < u64::MAX && get_hash_cash_level(&omega, offset) < level {
		offset += 1;
	}
	offset
}

#[inline]
pub fn get_hash_cash_level(omega: &str, offset: u64) -> u8 {
	let data = Sha1::digest(format!("{}{}", omega, offset).as_bytes());
	let mut res = 0;
	for &d in data.as_slice() {
		if d == 0 {
			res += 8;
		} else {
			res += d.trailing_zeros() as u8;
			break;
		}
	}
	res
}

pub fn biguint_to_array(i: &BigUint) -> [u8; 64] {
	let mut v = i.to_bytes_le();

	// Extend with zeroes until 64 bytes
	let len = v.len();
	v.append(&mut vec![0; 64 - len]);
	v.reverse();

	let mut a = [0; 64];
	a.copy_from_slice(&v);
	a
}

pub fn array_to_biguint(i: &[u8; 64]) -> BigUint { BigUint::from_bytes_be(i) }

#[cfg(test)]
mod tests {
	use base64::prelude::*;

	use super::*;
	use crate::license::Licenses;
	use crate::packets::PacketType;
	use crate::utils;
	use tsproto_types::crypto::EccKeyPubEd25519;

	#[test]
	fn test_fake_crypt() {
		let data = (0..100).into_iter().collect::<Vec<_>>();
		let mut packet = OutPacket::new_with_dir(Direction::C2S, Flags::empty(), PacketType::Ack);
		packet.data_mut().extend_from_slice(&data);
		encrypt_fake(&mut packet).unwrap();
		let packet =
			InPacket::try_new(Direction::C2S, packet.data_mut().as_slice().into()).unwrap();
		let dec_data = decrypt_fake(&packet).unwrap();
		assert_eq!(&data, &dec_data);
	}

	#[test]
	fn test_fake_encrypt() {
		let mut packet = OutAck::new(Direction::C2S, PacketType::Command, 0);
		encrypt_fake(&mut packet).unwrap();

		let real_res: &[u8] =
			&[0xa4, 0x7b, 0x47, 0x94, 0xdb, 0xa9, 0x6a, 0xc5, 0, 0, 0, 0, 0x6, 0xfe, 0x18];
		assert_eq!(real_res, packet.data_mut().as_slice());
	}

	#[test]
	fn shared_iv31() {
		let licenses = Licenses::parse_ignore_expired(BASE64_STANDARD.decode("AQA1hUFJiiSs\
			0wFXkYuPUJVcDa6XCrZTcsvkB0Ffzz4CmwIITRXgCqeTYAcAAAAgQW5vbnltb3VzAAC\
			4R+5mos+UQ/KCbkpQLMI5WRp4wkQu8e5PZY4zU+/FlyAJwaE8CcJJ/A==")
			.unwrap()).unwrap();
		let derived_key =
			licenses.derive_public_key(EccKeyPubEd25519::from_bytes(crate::ROOT_KEY)).unwrap();

		let client_ek = [
			0xb0, 0x4e, 0xa1, 0xd9, 0x5c, 0x72, 0x64, 0xdf, 0x0d, 0xe8, 0xb3, 0x6b, 0xaa, 0x7c,
			0xa1, 0x5f, 0x75, 0x71, 0xf5, 0x1f, 0xa0, 0x54, 0xb5, 0x51, 0x27, 0x08, 0x8e, 0xdd,
			0x96, 0x3d, 0x6e, 0x79,
		];

		let priv_key = EccKeyPrivEd25519::from_bytes(client_ek);

		let alpha_b64 = BASE64_STANDARD.decode("Jkxq1wIvvhzaCA==").unwrap();
		let mut alpha = [0; 10];
		alpha.copy_from_slice(&alpha_b64);
		let beta_b64 = BASE64_STANDARD
			.decode("wU5T/MM6toW6Wge9th7VlTlzVZ9JDWypw2P9migfc25pjGP2Tj7Hm6rJpmKeHRr08Ch7BEAR")
			.unwrap();
		let mut beta = [0; 54];
		beta.copy_from_slice(&beta_b64);

		let expected_shared_shared_iv: [u8; 64] = [
			0x58, 0x78, 0xae, 0x08, 0x08, 0x72, 0x05, 0xb0, 0x13, 0x27, 0x10, 0xe9, 0x81, 0xb4,
			0xaf, 0x14, 0x14, 0x71, 0xad, 0xcd, 0x82, 0x98, 0xf3, 0xd1, 0x1d, 0x07, 0x20, 0x72,
			0x7e, 0xb2, 0x1b, 0x89, 0x47, 0x82, 0x1e, 0xfb, 0x02, 0x53, 0x5a, 0x8a, 0x52, 0x4d,
			0x9a, 0x7a, 0x09, 0x2c, 0x1b, 0xe7, 0x1f, 0xd1, 0x9d, 0x2a, 0x9d, 0x4f, 0xbd, 0xe3,
			0x22, 0x09, 0xe4, 0x86, 0x7d, 0x63, 0x49, 0x07,
		];

		let expected_xored_shared_shared_iv: [u8; 64] = [
			0x7e, 0x34, 0xc4, 0xdf, 0x0a, 0x5d, 0xbb, 0xac, 0xc9, 0x2f, 0xd1, 0xa7, 0xd2, 0x48,
			0x6c, 0x2e, 0xa2, 0xf4, 0x17, 0x97, 0x85, 0x25, 0x45, 0xcf, 0xc8, 0x92, 0x19, 0x01,
			0x2b, 0x2d, 0x52, 0x84, 0x2b, 0x2b, 0xdd, 0x98, 0xff, 0xc9, 0x72, 0x95, 0x21, 0x23,
			0xf3, 0xf6, 0x6a, 0xda, 0x55, 0xd9, 0xd8, 0x4a, 0x37, 0xe3, 0x3b, 0x2d, 0x23, 0xfe,
			0x38, 0xfd, 0x14, 0xae, 0x06, 0x67, 0x09, 0x16,
		];

		let (mut shared_iv, _shared_mac) = compute_iv_mac(&alpha, &beta, &priv_key, &derived_key);

		assert_eq!(&shared_iv as &[u8], &expected_xored_shared_shared_iv as &[u8]);

		for i in 0..10 {
			shared_iv[i] ^= alpha[i];
		}
		for i in 0..54 {
			shared_iv[i + 10] ^= beta[i];
		}

		assert_eq!(&shared_iv as &[u8], &expected_shared_shared_iv as &[u8]);

		let mut temp = [0; 70];
		temp[0] = 0x31;
		temp[1] = 0x2 & 0xf;
		temp[6..].copy_from_slice(&expected_xored_shared_shared_iv);

		let temporary: [u8; 70] = [
			0x31, 0x02, 0x00, 0x00, 0x00, 0x00, 0x7e, 0x34, 0xc4, 0xdf, 0x0a, 0x5d, 0xbb, 0xac,
			0xc9, 0x2f, 0xd1, 0xa7, 0xd2, 0x48, 0x6c, 0x2e, 0xa2, 0xf4, 0x17, 0x97, 0x85, 0x25,
			0x45, 0xcf, 0xc8, 0x92, 0x19, 0x01, 0x2b, 0x2d, 0x52, 0x84, 0x2b, 0x2b, 0xdd, 0x98,
			0xff, 0xc9, 0x72, 0x95, 0x21, 0x23, 0xf3, 0xf6, 0x6a, 0xda, 0x55, 0xd9, 0xd8, 0x4a,
			0x37, 0xe3, 0x3b, 0x2d, 0x23, 0xfe, 0x38, 0xfd, 0x14, 0xae, 0x06, 0x67, 0x09, 0x16,
		];
		assert!(&temp as &[u8] == &temporary as &[u8]);

		let keynonce = Sha256::digest(&temp);

		let expected_keynonce: [u8; 32] = [
			0xf3, 0x70, 0xd3, 0x43, 0xe7, 0x78, 0x15, 0x70, 0x7a, 0xff, 0x60, 0x48, 0xfb, 0xd9,
			0xac, 0x6b, 0xb6, 0x33, 0x35, 0x79, 0x31, 0x9b, 0x88, 0x0e, 0x2d, 0x25, 0xef, 0x9c,
			0xe9, 0x9e, 0x77, 0x5c,
		];

		assert!(keynonce.as_slice() == &expected_keynonce as &[u8]);
	}

	#[test]
	fn test_new_decrypt() {
		let shared_iv = utils::read_hex(
			"C2 45 6F CB FC 22 08 AE 44 2B 7D E7 3A 67 1B DA 93 09 B2 00 F2 CD 10 49 08 CD 3A B0 \
			 7B DD 58 AD",
		)
		.unwrap();
		let mut shared_iv = Sha512::digest(&shared_iv).as_slice().to_vec();
		assert_eq!(
			utils::read_hex(
				"4D 3F DA B7 D8 B0 2C 82 70 6A 39 3E 97 17 61 09 FA 03 AB 30 5C BB 78 7A 9A 82 D5 \
				 39 9A 60 FD A9 F6 7A D9 04 52 F2 AE 00 3E 35 E8 19 10 89 40 43 80 58 27 1F 0A E0 \
				 E0 3D E0 9C F0 A3 4D 15 6B F0"
			)
			.unwrap()
			.as_slice(),
			shared_iv.as_slice()
		);

		let alpha = [0; 10];
		let beta = BASE64_STANDARD
			.decode("I4onb0zMyAD6bd24QANDls40eOES7qmjonBFtt5wRWzAfIIQWTSxjEas6TGTZIJ8QSJNX+Pl")
			.unwrap();

		for i in 0..10 {
			shared_iv[i] ^= alpha[i];
		}
		for i in 0..54 {
			shared_iv[i + 10] ^= beta[i];
		}

		let mut temp = [0; 70];
		temp[0] = 0x31;
		temp[1] = 0x2 & 0xf;
		temp[6..].copy_from_slice(&shared_iv);

		let keynonce = Sha256::digest(&temp);
		let keynonce = keynonce.as_slice();

		let all = utils::read_hex("2b982443ab38be6b00020000329abf64d4572e1349897b5e1e96fbc4a763a4c4ce1f64f0c1e3febd0a5f04a82ab1f2bc2344bb374fd16181beb8233b5b06944280470e9b6893290a1da0776ffcd89f3beec2ce23b9694930c09efaaea0d88a6895a08ede4d5cbfea61291fc553ac651f1e2bc1d2bd277a8bd9ab5386415579a9e56fac46d8b6b119f454bebd99179cd317dec60af205341d11f274d02bbacdd7e9773f72a426358ca1d39016dd95bde2409cd81bf99b340887e997ea982370c6790cf4d23150460820224766838ea4ec4d71dd102ede701ea0001f392623aa410dd9ab0e45874da82e29e6e370515ec30a37dd73f5a364c233ff014384beab5f1708c9f48dfba33a520f8fcdcef055789c54693c3fe72c5bfaca7cb4ca1fed77b8624660b8abc882f4b95b1284cb6dc55019c6082dd6dd146fa50383662d7298bef04ababaf1af80e15cd4c1f81326f085788e2918e00324147dce39b23db71326abc3de4b94df10f1531e9cce202bba71fa3ebeefd77b21fa3260a62e92eeee2183421d384a8c48777e2f9efbc58d4f442c5f0529c7c0e27e81b2b6b1b05eb8fa19256886248d553582dfd24c7cfab3c3f7317a5cebc6504b53fa0e86fc8c1100fc1d506fcf96caa76a7c0b6a27e577f2efdecd4070e847a559bf37d75bfdbe9e814c702426ce696d8645bc300b5f28f9e7f1ce").unwrap();

		let packet = InPacket::try_new(Direction::C2S, &all).unwrap();
		println!("Packet: {:?}", packet);
		let mut key = [0; 16];
		let mut nonce = [0; 16];
		key.copy_from_slice(&keynonce[..16]);
		nonce.copy_from_slice(&keynonce[16..]);

		let expected_key =
			utils::read_hex("D2 42 75 71 9C EE 83 35 EF 8A CE E0 B7 28 40 B8").unwrap();
		let expected_nonce =
			utils::read_hex("9C D9 30 B7 58 FE 50 23 64 66 11 C5 36 0E A2 5F").unwrap();

		assert_eq!(&expected_nonce, &nonce);
		assert_eq!(&expected_key, &key);

		key[1] ^= 2; // packet id
		let dec = decrypt_key_nonce(&packet, &key.into(), &nonce.into()).unwrap();

		println!("Decrypted: {:?}", String::from_utf8_lossy(&dec));
	}
}
