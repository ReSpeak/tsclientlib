//! This module contains cryptography related code.
use std::convert::TryInto;
use std::{cmp, fmt, str};

use base64::prelude::*;
use curve25519_dalek_ng::constants;
use curve25519_dalek_ng::edwards::{CompressedEdwardsY, EdwardsPoint};
use curve25519_dalek_ng::scalar::Scalar;
use elliptic_curve::sec1::{FromEncodedPoint, ToEncodedPoint};
use generic_array::typenum::Unsigned;
use generic_array::GenericArray;
use num_bigint::{BigInt, Sign};
use p256::ecdsa::signature::{Signer, Verifier};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha1::{Digest, Sha1};
use simple_asn1::ASN1Block;
use thiserror::Error;

type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, Clone, Error)]
#[non_exhaustive]
pub enum Error {
	#[error("More than one ASN.1 block")]
	TooManyAsn1Blocks,
	#[error("Invalid ASN.1: Expected a public key, not a private key")]
	UnexpectedPrivateKey,
	#[error("Invalid ASN.1: Does not contain a private key")]
	NoPrivateKey,
	#[error("Invalid ASN.1: Public key not found")]
	PublicKeyNotFound,
	#[error("Invalid ASN.1: Expected a bitstring")]
	ExpectedBitString,
	#[error("Invalid ASN.1: Expected a sequence")]
	ExpectedSequence,
	#[error("Key data is empty")]
	EmptyKeyData,
	#[error("Any known methods to decode the key failed")]
	KeyDecodeError,
	#[error("Failed to parse short private key")]
	NoShortKey,
	#[error("Not a obfuscated TeamSpeak key")]
	NoObfuscatedKey,
	#[error("Found no initial 'V' with a valid number before")]
	NoCounterBlock,
	#[error("Failed to parse public key")]
	ParsePublicKeyFailed,
	#[error("Wrong key length")]
	WrongKeyLength,
	#[error("Failed to parse public key, expected length {expected} but got {got}")]
	WrongPublicKeyLength { expected: usize, got: usize },
	#[error("Wrong signature")]
	WrongSignature { key: EccKeyPubP256, data: Vec<u8>, signature: Vec<u8> },

	#[error(transparent)]
	Asn1Decode(#[from] simple_asn1::ASN1DecodeErr),
	#[error(transparent)]
	Asn1Encode(#[from] simple_asn1::ASN1EncodeErr),
	#[error(transparent)]
	Base64(#[from] base64::DecodeError),
	#[error(transparent)]
	Utf8(#[from] std::str::Utf8Error),
}

/// Xored onto saved identities in the TeamSpeak client settings file.
const IDENTITY_OBFUSCATION: [u8; 128] = *b"b9dfaa7bee6ac57ac7b65f1094a1c155\
	e747327bc2fe5d51c512023fe54a280201004e90ad1daaae1075d53b7d571c30e063b5a\
	62a4a017bb394833aa0983e6e";

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum KeyType {
	Public,
	Private,
}

/// A public ecc key.
///
/// The curve of this key is P-256, or PRIME256v1 as it is called by openssl.
#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
pub struct EccKeyPubP256(
	#[serde(
		deserialize_with = "deserialize_ecc_key_pub_p256",
		serialize_with = "serialize_ecc_key_pub_p256"
	)]
	p256::PublicKey,
);
/// A private ecc key.
///
/// The curve of this key is P-256, or PRIME256v1 as it is called by openssl.
#[derive(Clone)]
pub struct EccKeyPrivP256(p256::SecretKey);

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

/// Passwords are encoded as base64(sha1(password)).
pub fn encode_password(password: &[u8]) -> String {
	BASE64_STANDARD.encode(Sha1::digest(password).as_slice())
}

fn deserialize_ecc_key_pub_p256<'de, D: Deserializer<'de>>(
	de: D,
) -> Result<p256::PublicKey, D::Error> {
	let data: Vec<u8> = Deserialize::deserialize(de)?;
	Ok(EccKeyPubP256::from_short(&data).map_err(serde::de::Error::custom)?.0)
}

fn serialize_ecc_key_pub_p256<S: Serializer>(
	key: &p256::PublicKey, ser: S,
) -> Result<S::Ok, S::Error> {
	Serialize::serialize(&EccKeyPubP256(*key).to_short().as_slice(), ser)
}

impl fmt::Debug for EccKeyPubP256 {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "EccKeyPubP256({})", self.to_ts())
	}
}

impl fmt::Debug for EccKeyPrivP256 {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "EccKeyPrivP256({})", BASE64_STANDARD.encode(self.to_short()))
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
	pub fn from_short(data: &[u8]) -> Result<Self> {
		p256::PublicKey::from_sec1_bytes(data).map_err(|_| Error::ParsePublicKeyFailed).map(Self)
	}

	/// From base64 encoded tomcrypt key.
	pub fn from_ts(data: &str) -> Result<Self> {
		Self::from_tomcrypt(&BASE64_STANDARD.decode(data)?)
	}

	/// Decodes the public key from an ASN.1 DER object how tomcrypt stores it.
	///
	/// The format is:
	/// - `BitString` where the first bit is 1 if the private key is contained
	/// - `Integer`: The key size (32)
	/// - `Integer`: X coordinate of the public key
	/// - `Integer`: Y coordinate of the public key
	pub fn from_tomcrypt(data: &[u8]) -> Result<Self> {
		let blocks = simple_asn1::from_der(data)?;
		if blocks.len() != 1 {
			return Err(Error::TooManyAsn1Blocks);
		}
		if let ASN1Block::Sequence(_, blocks) = &blocks[0] {
			if let Some(ASN1Block::BitString(_, len, content)) = blocks.first() {
				if *len != 1 || content[0] & 0x80 != 0 {
					return Err(Error::UnexpectedPrivateKey);
				}
				if let (Some(ASN1Block::Integer(_, x)), Some(ASN1Block::Integer(_, y))) =
					(blocks.get(2), blocks.get(3))
				{
					let x_bytes = x.to_bytes_be().1;
					let y_bytes = y.to_bytes_be().1;
					let field_size = elliptic_curve::FieldBytesSize::<p256::NistP256>::to_usize();
					if x_bytes.len() != field_size {
						return Err(Error::WrongPublicKeyLength {
							expected: field_size,
							got: x_bytes.len(),
						});
					}
					if y_bytes.len() != field_size {
						return Err(Error::WrongPublicKeyLength {
							expected: field_size,
							got: y_bytes.len(),
						});
					}
					let enc_point = p256::EncodedPoint::from_affine_coordinates(
						GenericArray::from_slice(x_bytes.as_slice()),
						GenericArray::from_slice(y_bytes.as_slice()),
						false,
					);
					let enc_point = p256::PublicKey::from_encoded_point(&enc_point);
					if enc_point.is_some().into() {
						Ok(Self(enc_point.unwrap()))
					} else {
						Err(Error::ParsePublicKeyFailed)
					}
				} else {
					Err(Error::PublicKeyNotFound)
				}
			} else {
				Err(Error::ExpectedBitString)
			}
		} else {
			Err(Error::ExpectedSequence)
		}
	}

	/// Convert to base64 encoded public tomcrypt key.
	pub fn to_ts(&self) -> String { BASE64_STANDARD.encode(self.to_tomcrypt()) }

	pub fn to_tomcrypt(&self) -> Vec<u8> {
		let enc_point = self.0.as_affine().to_encoded_point(false);
		// We can unwrap here, creating the public key ensures that it is not the identity point,
		// which is the only time this returns `None`.
		let pubkey_x = BigInt::from_bytes_be(Sign::Plus, enc_point.x().unwrap());
		let pubkey_y = BigInt::from_bytes_be(Sign::Plus, enc_point.y().unwrap());

		// Only returns an error when encoding wrong objects, so fine to unwrap.
		simple_asn1::to_der(&ASN1Block::Sequence(0, vec![
			ASN1Block::BitString(0, 1, vec![0]),
			ASN1Block::Integer(0, 32.into()),
			ASN1Block::Integer(0, pubkey_x),
			ASN1Block::Integer(0, pubkey_y),
		]))
		.unwrap()
	}

	/// Get the SEC1 encoding of the public key point on the curve.
	pub fn to_short(&self) -> Vec<u8> {
		// TODO Maybe compress?
		self.0.as_affine().to_encoded_point(false).as_bytes().to_vec()
	}

	/// Compute the uid of this key without encoding it in base64.
	///
	/// returns sha1(ts encoded key)
	pub fn get_uid_no_base64(&self) -> Vec<u8> {
		Sha1::digest(self.to_ts().as_bytes()).as_slice().to_vec()
	}

	/// Compute the uid of this key.
	///
	/// Uid = base64(sha1(ts encoded key))
	pub fn get_uid(&self) -> String { BASE64_STANDARD.encode(self.get_uid_no_base64()) }

	pub fn verify(&self, data: &[u8], signature: &[u8]) -> Result<()> {
		let sig =
			p256::ecdsa::Signature::from_der(signature).map_err(|_| Error::WrongSignature {
				key: self.clone(),
				data: data.to_vec(),
				signature: signature.to_vec(),
			})?;
		let key = p256::ecdsa::VerifyingKey::from(&self.0);
		key.verify(data, &sig).map_err(|_| Error::WrongSignature {
			key: self.clone(),
			data: data.to_vec(),
			signature: signature.to_vec(),
		})
	}

	// For the bookkeeping
	#[allow(clippy::should_implement_trait)]
	pub fn as_ref(&self) -> &Self { self }
}

impl EccKeyPrivP256 {
	/// Create a new key key pair.
	pub fn create() -> Self { Self(p256::SecretKey::random(&mut rand::thread_rng())) }

	/// Try to import the key from any of the known formats.
	pub fn import(data: &[u8]) -> Result<Self> {
		if data.is_empty() {
			return Err(Error::EmptyKeyData);
		}

		if let Ok(s) = str::from_utf8(data) {
			if let Ok(r) = Self::import_str(s) {
				return Ok(r);
			}
		}
		if let Ok(r) = Self::from_tomcrypt(data) {
			return Ok(r);
		}
		if let Ok(r) = Self::from_short(data) {
			return Ok(r);
		}
		Err(Error::KeyDecodeError)
	}

	/// Try to import the key from any of the known formats.
	pub fn import_str(s: &str) -> Result<Self> {
		if let Ok(r) = BASE64_STANDARD.decode(s) {
			if let Ok(r) = Self::import(&r) {
				return Ok(r);
			}
		}
		if let Ok(r) = Self::from_ts_obfuscated(s) {
			return Ok(r);
		}
		Err(Error::KeyDecodeError)
	}

	/// The shortest format of a private key.
	///
	/// This is just the `BigNum` of the private key.
	pub fn from_short(data: &[u8]) -> Result<Self> {
		// TODO !! p256::SecretKey::from_bytes panics when the data is not 32 long !!
		// maybe create a pull request for that because das not good?!
		if data.len() != 32 {
			Err(Error::NoShortKey)
		} else {
			Ok(Self(
				p256::SecretKey::from_bytes(p256::FieldBytes::from_slice(data))
					.map_err(|_| Error::NoShortKey)?,
			))
		}
	}

	/// The shortest format of a private key.
	///
	/// This is just the `BigNum` of the private key.
	pub fn to_short(&self) -> elliptic_curve::FieldBytes<p256::NistP256> { self.0.to_bytes() }

	/// From base64 encoded tomcrypt key.
	pub fn from_ts(data: &str) -> Result<Self> {
		Self::from_tomcrypt(&BASE64_STANDARD.decode(data)?)
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
		let mut data = BASE64_STANDARD.decode(data)?;
		if data.len() < 20 {
			return Err(Error::NoObfuscatedKey);
		}
		// Hash everything until the first 0 byte, starting after the first 20
		// bytes.
		let pos = data[20..].iter().position(|b| *b == b'\0').unwrap_or(data.len() - 20);
		let hash = Sha1::digest(&data[20..20 + pos]);
		let hash = hash.as_slice();
		// Xor first 20 bytes of data with the hash
		for i in 0..20 {
			data[i] ^= hash[i];
		}

		// Xor first 100 bytes with a static value
		#[allow(clippy::needless_range_loop)]
		for i in 0..cmp::min(data.len(), 100) {
			data[i] ^= IDENTITY_OBFUSCATION[i];
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
		let blocks = simple_asn1::from_der(data)?;
		if blocks.len() != 1 {
			return Err(Error::TooManyAsn1Blocks);
		}
		if let ASN1Block::Sequence(_, blocks) = &blocks[0] {
			if let Some(ASN1Block::BitString(_, len, content)) = blocks.first() {
				if (*len != 1 && *len != 2) || content[0] & 0x80 == 0 {
					return Err(Error::NoPrivateKey);
				}
				if *len == 1 {
					if let Some(ASN1Block::Integer(_, i)) = blocks.get(4) {
						Self::from_short(&i.to_bytes_be().1)
					} else {
						Err(Error::NoPrivateKey)
					}
				} else if let Some(ASN1Block::Integer(_, i)) = blocks.get(2) {
					Self::from_short(&i.to_bytes_be().1)
				} else {
					Err(Error::NoPrivateKey)
				}
			} else {
				Err(Error::ExpectedBitString)
			}
		} else {
			Err(Error::ExpectedSequence)
		}
	}

	/// Convert to base64 encoded private tomcrypt key.
	pub fn to_ts(&self) -> String { BASE64_STANDARD.encode(self.to_tomcrypt()) }

	/// Store as obfuscated TeamSpeak identity.
	pub fn to_ts_obfuscated(&self) -> String {
		let mut data = self.to_ts().into_bytes();
		// Xor first 100 bytes with a static value
		#[allow(clippy::needless_range_loop)]
		for i in 0..cmp::min(data.len(), 100) {
			data[i] ^= IDENTITY_OBFUSCATION[i];
		}

		// Hash everything until the first 0 byte, starting after the first 20
		// bytes.
		let pos = data[20..].iter().position(|b| *b == b'\0').unwrap_or(data.len() - 20);
		let hash = Sha1::digest(&data[20..20 + pos]);
		let hash = hash.as_slice();
		// Xor first 20 bytes of data with the hash
		for i in 0..20 {
			data[i] ^= hash[i];
		}
		BASE64_STANDARD.encode(data)
	}

	pub fn to_tomcrypt(&self) -> Vec<u8> {
		let enc_point = self.0.public_key().as_affine().to_encoded_point(false);
		// We can unwrap here, creating the public key ensures that it is not the identity point,
		// which is the only time this returns `None`.
		let pubkey_x = BigInt::from_bytes_be(Sign::Plus, enc_point.x().unwrap());
		let pubkey_y = BigInt::from_bytes_be(Sign::Plus, enc_point.y().unwrap());
		let privkey = BigInt::from_bytes_be(Sign::Plus, &self.0.to_bytes());

		// Only returns an error when encoding wrong objects, so fine to unwrap.
		simple_asn1::to_der(&ASN1Block::Sequence(0, vec![
			ASN1Block::BitString(0, 1, vec![0x80]),
			ASN1Block::Integer(0, 32.into()),
			ASN1Block::Integer(0, pubkey_x),
			ASN1Block::Integer(0, pubkey_y),
			ASN1Block::Integer(0, privkey),
		]))
		.unwrap()
	}

	/// This has to be the private key, the other one has to be the public key.
	pub fn create_shared_secret(
		self, other: EccKeyPubP256,
	) -> elliptic_curve::ecdh::SharedSecret<p256::NistP256> {
		elliptic_curve::ecdh::diffie_hellman(self.0.to_nonzero_scalar(), other.0.as_affine())
	}

	pub fn sign(self, data: &[u8]) -> Vec<u8> {
		let key = p256::ecdsa::SigningKey::from(self.0);
		let sig: p256::ecdsa::DerSignature = key.sign(data);
		sig.as_bytes().to_vec()
	}

	pub fn to_pub(&self) -> EccKeyPubP256 { self.into() }
}

impl<'a> From<&'a EccKeyPrivP256> for EccKeyPubP256 {
	fn from(priv_key: &'a EccKeyPrivP256) -> Self { Self(priv_key.0.public_key()) }
}

impl EccKeyPubEd25519 {
	pub fn from_bytes(data: [u8; 32]) -> Self { EccKeyPubEd25519(CompressedEdwardsY(data)) }

	pub fn from_base64(data: &str) -> Result<Self> {
		let decoded = BASE64_STANDARD.decode(data)?;
		if decoded.len() != 32 {
			return Err(Error::WrongKeyLength);
		}
		Ok(Self::from_bytes(decoded[..32].try_into().unwrap()))
	}

	pub fn to_base64(&self) -> String {
		let EccKeyPubEd25519(CompressedEdwardsY(ref data)) = *self;
		BASE64_STANDARD.encode(data)
	}
}

impl EccKeyPrivEd25519 {
	/// This is not used to create TeamSpeak keys, as they are not canonical.
	pub fn create() -> Self { EccKeyPrivEd25519(Scalar::random(&mut rand::thread_rng())) }

	pub fn from_base64(data: &str) -> Result<Self> {
		let decoded = BASE64_STANDARD.decode(data)?;
		if decoded.len() != 32 {
			return Err(Error::WrongKeyLength);
		}
		Ok(Self::from_bytes(decoded[..32].try_into().unwrap()))
	}

	pub fn from_bytes(data: [u8; 32]) -> Self {
		EccKeyPrivEd25519(Scalar::from_bytes_mod_order(data))
	}

	pub fn to_base64(&self) -> String { BASE64_STANDARD.encode(self.0.as_bytes()) }

	/// This has to be the private key, the other one has to be the public key.
	pub fn create_shared_secret(&self, pub_key: &EdwardsPoint) -> [u8; 32] {
		let res = pub_key * self.0;
		res.compress().0
	}

	pub fn to_pub(&self) -> EccKeyPubEd25519 { self.into() }
}

impl<'a> From<&'a EccKeyPrivEd25519> for EccKeyPubEd25519 {
	fn from(priv_key: &'a EccKeyPrivEd25519) -> Self {
		Self((&constants::ED25519_BASEPOINT_TABLE * &priv_key.0).compress())
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	const TEST_PRIV_KEY: &str = "MG0DAgeAAgEgAiAIXJBlj1hQbaH0Eq0DuLlCmH8bl+veTA\
		O2+k9EQjEYSgIgNnImcmKo7ls5mExb6skfK2Tw+u54aeDr0OP1ITsC/50CIA8M5nmDB\
		nmDM/gZ//4AAAAAAAAAAAAAAAAAAAAZRzOI";

	#[test]
	fn parse_p256_priv_key() { EccKeyPrivP256::from_ts(TEST_PRIV_KEY).unwrap(); }

	#[test]
	fn p256_ecdh() {
		let priv_key1 = EccKeyPrivP256::create();
		let pub_key1 = priv_key1.to_pub();
		let priv_key2 = EccKeyPrivP256::create();
		let pub_key2 = priv_key2.to_pub();

		let res1 = priv_key1.create_shared_secret(pub_key2);
		let res2 = priv_key2.create_shared_secret(pub_key1);
		assert_eq!(res1.raw_secret_bytes(), res2.raw_secret_bytes());
	}

	#[test]
	fn p256_signature() {
		let license =
			"AQBM0LZCVmZ7CX/\
			 miewqdjOyuKa6kI78Fk43LoypifqOkAIOkvUAEn46gAcAAAAgQW5vbnltb3VzAABoruUa34pO9zy1Z5zIOmrkIO06lKg/\
			 +mBrg6Mw1Rg4OyAPa7A3D2xY9w==";
		let server_key = "MEwDAgcAAgEgAiEA96WgYeYU8zoPqXJqicita+rR92FvnTlxYcUUyIDkQ6cCIE/\
		                  KPo+ms3BEzN/HBR71BJ/Z1Fv8918mdDKLetbOGKWt";
		let signature = "MEUCIQC+ececxC0NCcuCtrXHAO5h7qbh1s/TGP/\
		                 AaHa6+wV38wIgV9wwSppEdGjwuH3ETAME9tDj3aNkNvL25i0ikF9vs8M=";
		let license = BASE64_STANDARD.decode(license).unwrap();
		let signature = BASE64_STANDARD.decode(signature).unwrap();
		let server_key = EccKeyPubP256::from_ts(server_key).unwrap();
		server_key.verify(&license, &signature).unwrap();
	}

	#[test]
	fn obfuscated_priv_key() {
		let key = EccKeyPrivP256::from_ts(TEST_PRIV_KEY).unwrap();
		let obf = key.to_ts_obfuscated();
		let key2 = EccKeyPrivP256::from_ts_obfuscated(&obf).unwrap();
		assert_eq!(key.to_short(), key2.to_short());
	}

	#[test]
	fn obfuscated_identity() {
		let key = EccKeyPrivP256::from_ts(TEST_PRIV_KEY).unwrap();
		let uid = key.to_pub().get_uid();

		let expected_uid = "lks7QL5OVMKo4pZ79cEOI5r5oEA=";
		assert_eq!(expected_uid, &uid);
	}

	#[test]
	fn tsaudiobot_identity() {
		let key = EccKeyPrivP256::import_str(
			"MCkDAgbAAgEgAiBhPImh+bO1xMGOrcplwN3G74bhE9XATm+DxVo3aNtBqg==",
		)
		.unwrap();
		let uid = key.to_pub().get_uid();
		let expected_uid = "test/9PZ9vww/Bpf5vJxtJhpz80=";
		assert_eq!(expected_uid, &uid);
	}

	#[test]
	fn test_p256_priv_key_short() {
		let key = EccKeyPrivP256::from_ts(TEST_PRIV_KEY).unwrap();
		let short = key.to_short();
		let key = EccKeyPrivP256::from_short(short.as_slice()).unwrap();
		let short2 = key.to_short();
		assert_eq!(short, short2);
	}

	#[test]
	fn parse_ed25519_pub_key() {
		EccKeyPubEd25519::from_base64("zQ3irtRjRVCafjz9j2iz3HVVsp3M7HPNGHUPmTgSQIo=").unwrap();
	}
}
