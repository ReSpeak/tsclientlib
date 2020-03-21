//! This module contains cryptography related code.
use std::{cmp, fmt, str};

use anyhow::format_err;
use arrayref::array_ref;
use base64;
use curve25519_dalek::constants;
use curve25519_dalek::edwards::{CompressedEdwardsY, EdwardsPoint};
use curve25519_dalek::scalar::Scalar;
use num_bigint::{BigInt, Sign};
use ring::digest;
use ring::signature::KeyPair;
use simple_asn1::ASN1Block;
use untrusted::Input;

use crate::BasicError;

type Result<T> = std::result::Result<T, BasicError>;

pub enum KeyType {
	Public,
	Private,
}

/// A public ecc key.
///
/// The curve of this key is P-256, or PRIME256v1 as it is called by openssl.
#[derive(Clone)]
pub struct EccKeyPubP256(Vec<u8>);
/// A private ecc key.
///
/// The curve of this key is P-256, or PRIME256v1 as it is called by openssl.
#[derive(Clone)]
pub struct EccKeyPrivP256(Vec<u8>);

/// A public ecc key.
///
/// The curve of this key is Ed25519.
#[derive(Clone)]
pub struct EccKeyPubEd25519(pub CompressedEdwardsY);
/// A private ecc key.
///
/// The curve of this key is Ed25519.
#[derive(Clone)]
pub struct EccKeyPrivEd25519(pub Scalar);

impl fmt::Debug for EccKeyPubP256 {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "EccKeyPubP256({})", self.to_ts().unwrap())
	}
}

impl fmt::Debug for EccKeyPrivP256 {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "EccKeyPrivP256({})", base64::encode(&self.to_short()))
	}
}

impl fmt::Debug for EccKeyPubEd25519 {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "EccKeyPubEd25519({})", self.to_base64())
	}
}

impl fmt::Debug for EccKeyPrivEd25519 {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "EccKeyPrivEd25519({})", self.to_base64())
	}
}

impl EccKeyPubP256 {
	/// The shortest format of a public key.
	///
	/// This is just the `BigNum` of the x and y coordinates concatenated in
	/// this order. Each of the coordinates takes half of the size.
	pub fn from_short(data: Vec<u8>) -> Self { Self(data) }

	/// From base64 encoded tomcrypt key.
	pub fn from_ts(data: &str) -> Result<Self> {
		Self::from_tomcrypt(&base64::decode(data)?)
	}

	/// Decodes the public key from an ASN.1 DER object how tomcrypt stores it.
	///
	/// The format is:
	/// - `BitString` where the first bit is 1 if the private key is contained
	/// - `Integer`: The key size (32)
	/// - `Integer`: X coordinate of the public key
	/// - `Integer`: Y coordinate of the public key
	pub fn from_tomcrypt(data: &[u8]) -> Result<Self> {
		let blocks = ::simple_asn1::from_der(data)?;
		if blocks.len() != 1 {
			return Err(format_err!("More than one ASN.1 block").into());
		}
		if let ASN1Block::Sequence(_, blocks) = &blocks[0] {
			if let Some(ASN1Block::BitString(_, len, content)) = blocks.get(0) {
				if *len != 1 || content[0] & 0x80 != 0 {
					return Err(format_err!(
						"Expected a public key, not a private key"
					)
					.into());
				}
				if let (
					Some(ASN1Block::Integer(_, x)),
					Some(ASN1Block::Integer(_, y)),
				) = (blocks.get(2), blocks.get(3))
				{
					// Store as uncompressed coordinates:
					// 0x04
					// x: 32 bytes
					// y: 32 bytes
					let mut data = Vec::with_capacity(65);
					data.push(4);
					data.extend_from_slice(&x.to_bytes_be().1);
					data.extend_from_slice(&y.to_bytes_be().1);
					Ok(EccKeyPubP256(data))
				} else {
					return Err(format_err!("Public key not found").into());
				}
			} else {
				return Err(format_err!("Expected a bitstring").into());
			}
		} else {
			return Err(format_err!("Expected a sequence").into());
		}
	}

	/// Convert to base64 encoded public tomcrypt key.
	pub fn to_ts(&self) -> Result<String> {
		Ok(base64::encode(&self.to_tomcrypt()?))
	}

	pub fn to_tomcrypt(&self) -> Result<Vec<u8>> {
		let pub_len = (self.0.len() - 1) / 2;
		let pubkey_x = BigInt::from_bytes_be(Sign::Plus, &self.0[1..=pub_len]);
		let pubkey_y =
			BigInt::from_bytes_be(Sign::Plus, &self.0[1 + pub_len..]);

		Ok(::simple_asn1::to_der(&ASN1Block::Sequence(0, vec![
			ASN1Block::BitString(0, 1, vec![0]),
			ASN1Block::Integer(0, 32.into()),
			ASN1Block::Integer(0, pubkey_x),
			ASN1Block::Integer(0, pubkey_y),
		]))?)
	}

	/// The shortest format of a public key.
	///
	/// This is just the `BigNum` of the x and y coordinates concatenated in
	/// this order. Each of the coordinates takes half of the size.
	pub fn to_short(&self) -> &[u8] { &self.0 }

	/// Compute the uid of this key without encoding it in base64.
	///
	/// returns sha1(ts encoded key)
	pub fn get_uid_no_base64(&self) -> Result<Vec<u8>> {
		let hash = digest::digest(
			&digest::SHA1_FOR_LEGACY_USE_ONLY,
			self.to_ts()?.as_bytes(),
		);
		Ok(hash.as_ref().to_vec())
	}

	/// Compute the uid of this key.
	///
	/// Uid = base64(sha1(ts encoded key))
	pub fn get_uid(&self) -> Result<String> {
		Ok(base64::encode(&self.get_uid_no_base64()?))
	}

	pub fn verify(&self, data: &[u8], signature: &[u8]) -> Result<()> {
		let key = ring::signature::UnparsedPublicKey::new(
			&ring::signature::ECDSA_P256_SHA256_ASN1,
			&self.0,
		);
		key.verify(data, signature).map_err(|_| BasicError::WrongSignature {
			key: self.clone(),
			data: data.to_vec(),
			signature: signature.to_vec(),
		})
	}
}

impl EccKeyPrivP256 {
	/// Create a new key key pair.
	pub fn create() -> Result<Self> {
		Ok(EccKeyPrivP256(
			ring::signature::EcdsaKeyPair::generate_private_key(
				&ring::signature::ECDSA_P256_SHA256_ASN1_SIGNING,
				&ring::rand::SystemRandom::new(),
			)
			.map_err(|_| format_err!("Failed to generate private key"))?,
		))
	}

	/// Try to import the key from any of the known formats.
	pub fn import(data: &[u8]) -> Result<Self> {
		if data.is_empty() {
			return Err(format_err!("Key data is empty").into());
		}

		if let Ok(s) = str::from_utf8(data) {
			if let Ok(r) = Self::import_str(s) {
				return Ok(r);
			}
		}
		if let Ok(r) = Self::from_tomcrypt(data) {
			return Ok(r);
		}
		if let Ok(r) = Self::from_short(data.to_vec()) {
			return Ok(r);
		}
		Err(format_err!("Any known methods to decode the key failed").into())
	}

	/// Try to import the key from any of the known formats.
	pub fn import_str(s: &str) -> Result<Self> {
		if let Ok(r) = base64::decode(s) {
			if let Ok(r) = Self::import(&r) {
				return Ok(r);
			}
		}
		if let Ok(r) = Self::from_ts_obfuscated(s) {
			return Ok(r);
		}
		Err(format_err!("Any known methods to decode the key failed").into())
	}

	/// The shortest format of a private key.
	///
	/// This is just the `BigNum` of the private key.
	pub fn from_short(data: Vec<u8>) -> Result<Self> {
		// Check if the key is valid by converting it to a ring key
		let _ = ring::signature::EcdsaKeyPair::from_private_key(
			&ring::signature::ECDSA_P256_SHA256_ASN1_SIGNING,
			Input::from(&data),
		)
		.map_err(|_| format_err!("Failed to parse private key"))?;
		Ok(Self(data))
	}

	/// The shortest format of a private key.
	///
	/// This is just the `BigNum` of the private key.
	pub fn to_short(&self) -> &[u8] { &self.0 }

	/// From base64 encoded tomcrypt key.
	pub fn from_ts(data: &str) -> Result<Self> {
		Self::from_tomcrypt(&base64::decode(data)?)
	}

	/// From the key representation which is used to store identities in the
	/// TeamSpeak configuration file.
	///
	/// Format: Offset for identity level || 'V' || obfuscated key
	///
	/// This function takes only the obfuscated key without the level.
	///
	/// Thanks to landave, who put
	/// [his deobfuscation code](https://github.com/landave/TSIdentityTool)
	/// under the MIT license.
	pub fn from_ts_obfuscated(data: &str) -> Result<Self> {
		let mut data = base64::decode(data)?;
		if data.len() < 20 {
			return Err(
				format_err!("Not a known obfuscated TeamSpeak key").into()
			);
		}
		// Hash everything until the first 0 byte, starting after the first 20
		// bytes.
		let pos = data[20..]
			.iter()
			.position(|b| *b == b'\0')
			.unwrap_or(data.len() - 20);
		let hash = digest::digest(
			&digest::SHA1_FOR_LEGACY_USE_ONLY,
			&data[20..20 + pos],
		);
		let hash = hash.as_ref();
		// Xor first 20 bytes of data with the hash
		for i in 0..20 {
			data[i] ^= hash[i];
		}

		// Xor first 100 bytes with a static value
		#[allow(clippy::needless_range_loop)]
		for i in 0..cmp::min(data.len(), 100) {
			data[i] ^= crate::IDENTITY_OBFUSCATION[i];
		}
		Self::from_ts(str::from_utf8(&data)?)
	}

	/// Decodes the private key from an ASN.1 DER object how tomcrypt stores it.
	///
	/// The format is:
	/// - `BitString` where the first bit is 1 if the private key is contained
	/// - `Integer`: The key size (32)
	/// - `Integer`: X coordinate of the public key
	/// - `Integer`: Y coordinate of the public key
	/// - `Integer`: Private key
	///
	/// The TS3AudioBot stores two 1 bits in the first `BitString` and omits the
	/// public key.
	pub fn from_tomcrypt(data: &[u8]) -> Result<Self> {
		let blocks = ::simple_asn1::from_der(data)?;
		if blocks.len() != 1 {
			return Err(format_err!("More than one ASN.1 block").into());
		}
		if let ASN1Block::Sequence(_, blocks) = &blocks[0] {
			if let Some(ASN1Block::BitString(_, len, content)) = blocks.get(0) {
				if (*len != 1 && *len != 2) || content[0] & 0x80 == 0 {
					return Err(format_err!(
						"Does not contain a private key ({}, {:?})",
						len,
						content
					)
					.into());
				}
				if *len == 1 {
					if let Some(ASN1Block::Integer(_, i)) = blocks.get(4) {
						Self::from_short(i.to_bytes_be().1)
					} else {
						return Err(format_err!("Private key not found").into());
					}
				} else if let Some(ASN1Block::Integer(_, i)) = blocks.get(2) {
					Self::from_short(i.to_bytes_be().1)
				} else {
					return Err(format_err!("Private key not found").into());
				}
			} else {
				return Err(format_err!("Expected a bitstring").into());
			}
		} else {
			return Err(format_err!("Expected a sequence").into());
		}
	}

	/// Convert to base64 encoded private tomcrypt key.
	pub fn to_ts(&self) -> Result<String> {
		Ok(base64::encode(&self.to_tomcrypt()?))
	}

	/// Store as obfuscated TeamSpeak identity.
	pub fn to_ts_obfuscated(&self) -> Result<String> {
		let mut data = self.to_ts()?.into_bytes();
		// Xor first 100 bytes with a static value
		#[allow(clippy::needless_range_loop)]
		for i in 0..cmp::min(data.len(), 100) {
			data[i] ^= crate::IDENTITY_OBFUSCATION[i];
		}

		// Hash everything until the first 0 byte, starting after the first 20
		// bytes.
		let pos = data[20..]
			.iter()
			.position(|b| *b == b'\0')
			.unwrap_or(data.len() - 20);
		let hash = digest::digest(
			&digest::SHA1_FOR_LEGACY_USE_ONLY,
			&data[20..20 + pos],
		);
		let hash = hash.as_ref();
		// Xor first 20 bytes of data with the hash
		for i in 0..20 {
			data[i] ^= hash[i];
		}
		Ok(base64::encode(&data))
	}

	pub fn to_tomcrypt(&self) -> Result<Vec<u8>> {
		let pubkey_bin = self.to_pub().0;
		let pub_len = (pubkey_bin.len() - 1) / 2;
		let pubkey_x =
			BigInt::from_bytes_be(Sign::Plus, &pubkey_bin[1..=pub_len]);
		let pubkey_y =
			BigInt::from_bytes_be(Sign::Plus, &pubkey_bin[1 + pub_len..]);

		let privkey = BigInt::from_bytes_be(Sign::Plus, &self.0);

		Ok(::simple_asn1::to_der(&ASN1Block::Sequence(0, vec![
			ASN1Block::BitString(0, 1, vec![0x80]),
			ASN1Block::Integer(0, 32.into()),
			ASN1Block::Integer(0, pubkey_x),
			ASN1Block::Integer(0, pubkey_y),
			ASN1Block::Integer(0, privkey),
		]))?)
	}

	/// Create a `ring` key from the stored private key.
	fn to_ring(&self) -> ring::signature::EcdsaKeyPair {
		ring::signature::EcdsaKeyPair::from_private_key(
			&ring::signature::ECDSA_P256_SHA256_ASN1_SIGNING,
			Input::from(&self.0),
		)
		.unwrap()
	}

	/// This has to be the private key, the other one has to be the public key.
	pub fn create_shared_secret(self, other: EccKeyPubP256) -> Result<Vec<u8>> {
		use ring::ec::keys::Seed;
		use ring::ec::suite_b::ecdh;

		let seed = Seed::from_p256_bytes(Input::from(&self.0))
			.map_err(|_| format_err!("Failed to parse public key"))?;
		let mut res = vec![0; 32];
		ecdh::p256_ecdh(&mut res, &seed, Input::from(&other.0))
			.map_err(|_| format_err!("Failed to complete key exchange"))?;
		Ok(res)
	}

	pub fn sign(self, data: &[u8]) -> Result<Vec<u8>> {
		let key = self.to_ring();
		Ok(key
			.sign(&ring::rand::SystemRandom::new(), data)
			.map_err(|_| format_err!("Failed to create signature"))?
			.as_ref()
			.to_vec())
	}

	pub fn to_pub(&self) -> EccKeyPubP256 { self.into() }
}

impl<'a> Into<EccKeyPubP256> for &'a EccKeyPrivP256 {
	fn into(self) -> EccKeyPubP256 {
		EccKeyPubP256(self.to_ring().public_key().as_ref().to_vec())
	}
}

impl EccKeyPubEd25519 {
	pub fn from_bytes(data: [u8; 32]) -> Self {
		EccKeyPubEd25519(CompressedEdwardsY(data))
	}

	pub fn from_base64(data: &str) -> Result<Self> {
		let decoded = base64::decode(data)?;
		if decoded.len() != 32 {
			return Err(format_err!("Wrong key length").into());
		}
		Ok(Self::from_bytes(*array_ref!(decoded, 0, 32)))
	}

	pub fn to_base64(&self) -> String {
		let EccKeyPubEd25519(CompressedEdwardsY(ref data)) = *self;
		base64::encode(data)
	}
}

impl EccKeyPrivEd25519 {
	/// This is not used to create TeamSpeak keys, as they are not canonical.
	pub fn create() -> Result<Self> {
		Ok(EccKeyPrivEd25519(Scalar::random(&mut rand::rngs::OsRng)))
	}

	pub fn from_base64(data: &str) -> Result<Self> {
		let decoded = base64::decode(data)?;
		if decoded.len() != 32 {
			return Err(format_err!("Wrong key length").into());
		}
		Ok(Self::from_bytes(*array_ref!(decoded, 0, 32)))
	}

	pub fn from_bytes(data: [u8; 32]) -> Self {
		EccKeyPrivEd25519(Scalar::from_bytes_mod_order(data))
	}

	pub fn to_base64(&self) -> String { base64::encode(self.0.as_bytes()) }

	/// This has to be the private key, the other one has to be the public key.
	pub fn create_shared_secret(
		&self, pub_key: &EdwardsPoint,
	) -> Result<[u8; 32]> {
		let res = pub_key * self.0;
		Ok(res.compress().0)
	}

	pub fn to_pub(&self) -> EccKeyPubEd25519 { self.into() }
}

impl<'a> Into<EccKeyPubEd25519> for &'a EccKeyPrivEd25519 {
	fn into(self) -> EccKeyPubEd25519 {
		EccKeyPubEd25519(
			(&constants::ED25519_BASEPOINT_TABLE * &self.0).compress(),
		)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	const TEST_PRIV_KEY: &str = "MG0DAgeAAgEgAiAIXJBlj1hQbaH0Eq0DuLlCmH8bl+veTA\
		O2+k9EQjEYSgIgNnImcmKo7ls5mExb6skfK2Tw+u54aeDr0OP1ITsC/50CIA8M5nmDB\
		nmDM/gZ//4AAAAAAAAAAAAAAAAAAAAZRzOI";

	#[test]
	fn parse_p256_priv_key() {
		EccKeyPrivP256::from_ts(TEST_PRIV_KEY).unwrap();
	}

	#[test]
	fn p256_ecdh() {
		let priv_key1 = EccKeyPrivP256::create().unwrap();
		let pub_key1 = priv_key1.to_pub();
		let priv_key2 = EccKeyPrivP256::create().unwrap();
		let pub_key2 = priv_key2.to_pub();

		let res1 = priv_key1.create_shared_secret(pub_key2).unwrap();
		let res2 = priv_key2.create_shared_secret(pub_key1).unwrap();
		assert_eq!(res1, res2);
	}

	#[test]
	fn obfuscated_priv_key() {
		let key = EccKeyPrivP256::from_ts(TEST_PRIV_KEY).unwrap();
		let obf = key.to_ts_obfuscated().unwrap();
		let key2 = EccKeyPrivP256::from_ts_obfuscated(&obf).unwrap();
		assert_eq!(key.to_short(), key2.to_short());
	}

	#[test]
	fn obfuscated_identity() {
		let key = EccKeyPrivP256::from_ts(TEST_PRIV_KEY).unwrap();
		let uid = key.to_pub().get_uid().unwrap();

		let expected_uid = "lks7QL5OVMKo4pZ79cEOI5r5oEA=";
		assert_eq!(expected_uid, &uid);
	}

	#[test]
	fn test_p256_priv_key_short() {
		let key = EccKeyPrivP256::from_ts(TEST_PRIV_KEY).unwrap();
		let short = key.to_short();
		let key = EccKeyPrivP256::from_short(short.to_vec()).unwrap();
		let short2 = key.to_short();
		assert_eq!(short, short2);
	}

	#[test]
	fn parse_ed25519_pub_key() {
		EccKeyPubEd25519::from_base64(
			"zQ3irtRjRVCafjz9j2iz3HVVsp3M7HPNGHUPmTgSQIo=",
		)
		.unwrap();
	}
}
