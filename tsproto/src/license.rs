use std::borrow::Cow;
use std::fmt;
use std::io::prelude::*;
use std::str;

use curve25519_dalek::constants;
use curve25519_dalek::edwards::EdwardsPoint;
use curve25519_dalek::scalar::Scalar;
use num_traits::{FromPrimitive as _, ToPrimitive as _};
use omnom::{ReadExt, WriteExt};
use sha2::{Digest, Sha512};
use thiserror::Error;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
use tsproto_types::crypto::{EccKeyPrivEd25519, EccKeyPubEd25519};
use tsproto_types::LicenseType;

pub const TIMESTAMP_OFFSET: i64 = 0x50e2_2700;

type Result<T> = std::result::Result<T, Error>;

#[derive(Error, Debug)]
#[non_exhaustive]
pub enum Error {
	#[error(
		"License must not exceed bounds {outer_start} - {outer_end} but has {inner_start} - \
		 {inner_end}"
	)]
	Bounds {
		outer_start: OffsetDateTime,
		outer_end: OffsetDateTime,
		inner_start: OffsetDateTime,
		inner_end: OffsetDateTime,
	},
	#[error("Failed to deserialize license: {0}")]
	Deserialize(#[source] std::io::Error),
	#[error("Failed to deserialize license: {0}")]
	DeserializeString(#[source] std::str::Utf8Error),
	#[error("Failed to deserialize date: {0}")]
	DeserializeDate(#[source] time::error::ComponentRange),
	#[error("License is only valid between {start} and {end}")]
	Expired { start: OffsetDateTime, end: OffsetDateTime },
	#[error("Cannot uncompress license public key")]
	InvalidPublicKey,
	#[error("Invalid data {0:#x} in intermediate license")]
	IntermediateInvalidData(u32),
	#[error("Invalid public root key for license")]
	InvalidRootKey,
	#[error("Non-null-terminated string")]
	NonterminatedString,
	#[error("License contains no private key")]
	NoPrivateKey,
	#[error("Failed to serialize license: {0}")]
	Serialize(#[source] std::io::Error),
	#[error("Too many license blocks: {0}")]
	TooManyBlocks(usize),
	#[error("License too short")]
	TooShort,
	#[error("Unknown license block type {0}")]
	UnknownBlockType(u8),
	#[error("Unknown license type {0}")]
	UnknownLicenseType(u8),
	#[error("Unsupported license version {0}")]
	UnsupportedVersion(u8),
	#[error("Wrong key kind {0} in license")]
	WrongKeyKind(u8),
}

/// Either a public or a private key.
#[derive(Debug, Clone)]
pub enum LicenseKey {
	Public(EccKeyPubEd25519),
	Private(EccKeyPrivEd25519),
}

#[derive(Clone)]
pub struct License {
	pub key: LicenseKey,
	pub not_valid_before: OffsetDateTime,
	pub not_valid_after: OffsetDateTime,
	/// First 32 byte of SHA512(last 4 bytes from key || rest of license block)
	pub hash: [u8; 32],
	pub inner: InnerLicense,
}

#[derive(Debug, Clone)]
pub enum InnerLicense {
	Intermediate {
		issuer: String,
		/// Unknown data
		data: u8,
	}, // 0
	Website {
		issuer: String,
	}, // 1
	Server {
		issuer: String,
		license_type: LicenseType,
		/// Unknown data
		data: u32,
	}, // 2
	Code {
		issuer: String,
	}, // 3
	// 4: Token, 5: LicenseSign, 6: MyTsIdSign (not existing in the license
	// parameter)
	Ephemeral, // 32
}

#[derive(Clone, Debug, Default)]
pub struct Licenses {
	pub blocks: Vec<License>,
}

impl LicenseKey {
	pub fn get_pub_bytes(&self) -> Cow<[u8; 32]> {
		match *self {
			LicenseKey::Public(ref k) => Cow::Borrowed(&(k.0).0),
			LicenseKey::Private(ref k) => Cow::Owned((k.to_pub().0).0),
		}
	}

	pub fn get_pub(&self) -> Result<EdwardsPoint> {
		match *self {
			LicenseKey::Public(ref k) => k.0.decompress().ok_or(Error::InvalidPublicKey),
			LicenseKey::Private(ref k) => Ok(&constants::ED25519_BASEPOINT_TABLE * &k.0),
		}
	}
}

impl InnerLicense {
	pub fn type_id(&self) -> u8 {
		match *self {
			InnerLicense::Intermediate { .. } => 0,
			InnerLicense::Website { .. } => 1,
			InnerLicense::Server { .. } => 2,
			InnerLicense::Code { .. } => 3,
			InnerLicense::Ephemeral { .. } => 32,
		}
	}
}

impl Licenses {
	pub fn new() -> Self { Self::default() }

	/// Parse a license but ignore expired licenses.
	///
	/// This is useful for tests but should not be used otherwise.
	pub fn parse_ignore_expired(data: &[u8]) -> Result<Self> { Self::parse_internal(data, false) }

	pub fn parse(data: &[u8]) -> Result<Self> { Self::parse_internal(data, true) }

	pub fn parse_internal(mut data: &[u8], check_expired: bool) -> Result<Self> {
		let version = data[0];
		if version != 0 && version != 1 {
			return Err(Error::UnsupportedVersion(version));
		}
		// Read licenses
		let mut res = Licenses { blocks: Vec::new() };
		data = &data[1..];

		let now = OffsetDateTime::now_utc();
		let mut bounds = None;
		while !data.is_empty() {
			if res.blocks.len() >= 8 {
				// Accept only 8 blocks
				return Err(Error::TooManyBlocks(res.blocks.len()));
			}

			// Read next license
			let (license, len) = License::parse(data)?;

			// Check if the certificate is valid
			if license.not_valid_before > now && check_expired {
				return Err(Error::Expired {
					start: license.not_valid_before,
					end: license.not_valid_after,
				});
			}
			if license.not_valid_after < now && check_expired {
				return Err(Error::Expired {
					start: license.not_valid_before,
					end: license.not_valid_after,
				});
			}
			if let Some((start, end)) = bounds {
				// The inner license must not have wider bounds
				if license.not_valid_before < start || license.not_valid_after > end {
					return Err(Error::Bounds {
						outer_start: start,
						outer_end: end,
						inner_start: license.not_valid_before,
						inner_end: license.not_valid_after,
					});
				}
			}
			bounds = Some((license.not_valid_before, license.not_valid_after));

			res.blocks.push(license);
			data = &data[len..];
		}
		Ok(res)
	}

	pub fn write(&self, mut w: &mut dyn Write) -> Result<()> {
		// Version
		w.write_be(1u8).map_err(Error::Serialize)?;

		for l in &self.blocks {
			l.write(w)?;
		}

		Ok(())
	}

	pub fn derive_public_key(&self, root: EccKeyPubEd25519) -> Result<EdwardsPoint> {
		let mut last_round = root.0.decompress().ok_or(Error::InvalidRootKey)?;
		for l in &self.blocks {
			//let derived_key = last_round.compress().0;
			//println!("Got key: {:?}", ::utils::HexSlice((&derived_key) as &[u8]));
			last_round = l.derive_public_key(&last_round)?;
		}
		//let derived_key = last_round.compress().0;
		//println!("Got end key: {:?}", ::utils::HexSlice((&derived_key) as &[u8]));
		Ok(last_round)
	}

	/// Derive the private key of this license, starting with a specific block.
	///
	/// The keys which are stored inside the licenses from the starting block on
	/// have to be private keys.
	///
	/// # Arguments
	///
	/// `starting_block`: The index of the first block for the key computation.
	/// `first_key`: The starting private key (the key of a parent block or the
	/// root private key).
	///
	/// # Panics
	///
	/// Panics if `starting_block` is not a valid license block index.
	pub fn derive_private_key(
		&self, starting_block: usize, first_key: EccKeyPrivEd25519,
	) -> Result<EccKeyPrivEd25519> {
		let mut res = first_key;
		for l in &self.blocks[starting_block..] {
			res = l.derive_private_key(&res)?;
		}
		Ok(res)
	}
}

impl License {
	/// Compute the hash of this license and store it in the hash field.
	///
	/// This is used when creating licenses.
	pub fn fill_hash(&mut self) {
		// Compute hash
		let mut data = Vec::new();
		self.write(&mut data).unwrap();
		let hash_out = Sha512::digest(&data[1..]);
		let mut hash = [0; 32];
		hash.copy_from_slice(&hash_out.as_slice()[..32]);
		self.hash = hash;
	}

	/// Parse license and return read length.
	pub fn parse(data: &[u8]) -> Result<(Self, usize)> {
		const MIN_LEN: usize = 42;

		if data.len() < MIN_LEN {
			return Err(Error::TooShort);
		}
		if data[0] != 0 {
			return Err(Error::WrongKeyKind(data[0]));
		}

		let mut key_data = [0; 32];
		key_data.copy_from_slice(&data[1..33]);

		let before_ts: u32 = (&data[34..]).read_be().map_err(Error::Deserialize)?;
		let after_ts: u32 = (&data[38..]).read_be().map_err(Error::Deserialize)?;

		let (inner, extra_len) = match data[33] {
			0 => {
				let license_data: u32 = (&data[42..]).read_be().map_err(Error::Deserialize)?;

				let len = if let Some(len) = data[46..].iter().position(|&b| b == 0) {
					len
				} else {
					return Err(Error::NonterminatedString);
				};
				let issuer = str::from_utf8(&data[46..46 + len])
					.map_err(Error::DeserializeString)?
					.to_string();
				(InnerLicense::Intermediate { issuer, data: license_data as u8 }, 5 + len)
			}
			2 => {
				if data.len() < 47 {
					return Err(Error::TooShort);
				}
				let license_type =
					LicenseType::from_u8(data[42]).ok_or(Error::UnknownLicenseType(data[42]))?;
				let license_data: u32 = (&data[43..]).read_be().map_err(Error::Deserialize)?;
				let len = if let Some(len) = data[47..].iter().position(|&b| b == 0) {
					len
				} else {
					return Err(Error::NonterminatedString);
				};
				if data.len() < 47 + len {
					return Err(Error::TooShort);
				}
				let issuer = str::from_utf8(&data[47..47 + len])
					.map_err(Error::DeserializeString)?
					.to_string();
				(InnerLicense::Server { issuer, license_type, data: license_data }, 6 + len)
			}
			8 => {
				// TeamSpeak 5 exclusive license block. Similar to server but with extra data
				if data.len() < 47 {
					return Err(Error::TooShort);
				}
				let license_type =
					LicenseType::from_u8(data[42]).ok_or(Error::UnknownLicenseType(data[42]))?;
				let license_data: u32 = (&data[43..]).read_be().map_err(Error::Deserialize)?;
				let len = if let Some(len) = data[47..].iter().position(|&b| b == 0) {
					len
				} else {
					return Err(Error::NonterminatedString);
				};
				if data.len() < 47 + len + 1 {
					return Err(Error::TooShort);
				}
				let issuer = str::from_utf8(&data[47..47 + len])
					.map_err(Error::DeserializeString)?
					.to_string();

				// There is an extra field with additional data
				let len2 = data[47 + len + 1] as usize;
				if data.len() < 48 + len + len2 {
					return Err(Error::TooShort);
				}
				(InnerLicense::Server { issuer, license_type, data: license_data }, 7 + len + len2)
			}
			32 => (InnerLicense::Ephemeral, 0),
			i => {
				return Err(Error::UnknownBlockType(i));
			}
		};

		let all_len = MIN_LEN + extra_len;
		let hash_out = Sha512::digest(&data[1..all_len]);
		let mut hash = [0; 32];
		hash.copy_from_slice(&hash_out.as_slice()[..32]);

		Ok((
			License {
				key: LicenseKey::Public(EccKeyPubEd25519::from_bytes(key_data)),
				not_valid_before: OffsetDateTime::from_unix_timestamp(
					i64::from(before_ts) + TIMESTAMP_OFFSET,
				)
				.map_err(Error::DeserializeDate)?,
				not_valid_after: OffsetDateTime::from_unix_timestamp(
					i64::from(after_ts) + TIMESTAMP_OFFSET,
				)
				.map_err(Error::DeserializeDate)?,
				hash,
				inner,
			},
			all_len,
		))
	}

	pub fn write(&self, mut w: &mut dyn Write) -> Result<()> {
		w.write_be(0u8).map_err(Error::Serialize)?;
		// Public key
		w.write_all(self.key.get_pub_bytes().as_ref()).map_err(Error::Serialize)?;
		// Type
		w.write_be(self.inner.type_id()).map_err(Error::Serialize)?;

		w.write_be((self.not_valid_before.unix_timestamp() - TIMESTAMP_OFFSET) as u32)
			.map_err(Error::Serialize)?;
		w.write_be((self.not_valid_after.unix_timestamp() - TIMESTAMP_OFFSET) as u32)
			.map_err(Error::Serialize)?;

		match self.inner {
			InnerLicense::Intermediate { ref issuer, data } => {
				w.write_be(u32::from(data)).map_err(Error::Serialize)?;
				w.write_all(issuer.as_bytes()).map_err(Error::Serialize)?;
				w.write_be(0u8).map_err(Error::Serialize)?;
			}
			InnerLicense::Website { ref issuer } => {
				w.write_all(issuer.as_bytes()).map_err(Error::Serialize)?;
				w.write_be(0u8).map_err(Error::Serialize)?;
			}
			InnerLicense::Server { ref issuer, license_type, data } => {
				w.write_be(license_type.to_u8().unwrap()).map_err(Error::Serialize)?;
				w.write_be(data).map_err(Error::Serialize)?;
				w.write_all(issuer.as_bytes()).map_err(Error::Serialize)?;
				w.write_be(0u8).map_err(Error::Serialize)?;
			}
			InnerLicense::Code { ref issuer } => {
				w.write_all(issuer.as_bytes()).map_err(Error::Serialize)?;
				w.write_be(0u8).map_err(Error::Serialize)?;
			}
			InnerLicense::Ephemeral => {}
		}

		Ok(())
	}

	/// Get the private key from the hash of this license block.
	pub fn get_hash_key(&self) -> Scalar {
		// Make a valid private key from the hash
		let mut hash_key = self.hash;
		hash_key[0] &= 248;
		hash_key[31] &= 63;
		hash_key[31] |= 64;
		Scalar::from_bytes_mod_order(hash_key)
	}

	pub fn derive_public_key(&self, parent_key: &EdwardsPoint) -> Result<EdwardsPoint> {
		let hash_key = self.get_hash_key();
		let pub_key = self.key.get_pub()?;
		Ok(pub_key * hash_key + parent_key)
	}

	/// Derive the private key of this license block.
	///
	/// The key which is stored inside this license has to be a private key.
	///
	/// # Arguments
	///
	/// `parent_key`: The resulting private key of the previous block.
	pub fn derive_private_key(&self, parent_key: &EccKeyPrivEd25519) -> Result<EccKeyPrivEd25519> {
		let priv_key = if let LicenseKey::Private(ref k) = self.key {
			&k.0
		} else {
			return Err(Error::NoPrivateKey);
		};
		let hash_key = self.get_hash_key();
		Ok(EccKeyPrivEd25519(priv_key * hash_key + parent_key.0))
	}
}

impl fmt::Debug for License {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "License {{ key: ")?;
		match &self.key {
			LicenseKey::Public(k) => write!(f, "{:?}", k)?,
			LicenseKey::Private(k) => write!(f, "{:?}", k)?,
		}
		let from = self.not_valid_before.format(&Rfc3339).unwrap_or_else(|_| "Invalid".to_string());
		let to = self.not_valid_after.format(&Rfc3339).unwrap_or_else(|_| "Invalid".to_string());
		write!(f, ", valid_between: {} - {}, ", from, to)?;
		write!(f, "inner: {:?} }}", self.inner)?;
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use base64;

	#[test]
	fn parse_standard_license() {
		Licenses::parse_ignore_expired(&base64::decode("AQA1hUFJiiSs0wFXkYuPUJVcDa6XCrZTcsvkB0\
			Ffzz4CmwIITRXgCqeTYAcAAAAgQW5vbnltb3VzAACiIBip9hQaK6P3QhwOJs/BkPn0i\
			oyIDPaNgzJ6M8x0kiAJf4hxCYAxMQ==").unwrap()).unwrap();
	}

	#[test]
	fn parse_standard_license_expired() {
		assert!(Licenses::parse(&base64::decode("AQA1hUFJiiSs0wFXkYuPUJVcDa6XCrZTcsvkB0\
			Ffzz4CmwIITRXgCqeTYAcAAAAgQW5vbnltb3VzAACiIBip9hQaK6P3QhwOJs/BkPn0i\
			oyIDPaNgzJ6M8x0kiAJf4hxCYAxMQ==").unwrap()).is_err());
	}

	#[test]
	fn parse_aal_license() {
		Licenses::parse_ignore_expired(&base64::decode("AQCvbHFTQDY/terPeilrp/ECU9xCH5U3xC92lY\
			TNaY/0KQAJFueAazbsgAAAACVUZWFtU3BlYWsgU3lzdGVtcyBHbWJIAABhl9gwla/UJ\
			p2Eszst9TRVXO/PeE6a6d+CTI6Pg7OEVgAJc5CrL4Nh8gAAACRUZWFtU3BlYWsgc3lz\
			dGVtcyBHbWJIAACvTQIgpv6zmLZq3znh7ygmOSokGFkFjz4bTigrOnetrgIJdIIACdS\
			/gAYAAAAAU29zc2VuU3lzdGVtcy5iaWQAADY7+uV1CQ1niOvYSdGzsu83kPTNWijovr\
			3B78eHGeePIAm98vQJvpu0").unwrap()).unwrap();
	}

	#[test]
	fn parse_ts5_server_license() {
		Licenses::parse_ignore_expired(&base64::decode("AQDVsMGbcrMmGif1vSXPWWXNW2CB5\
			Fe9oZ/2uxP29j1EXQAQSfiAazbsgAAAASVUZWFtU3BlYWsgU3lzdGVtcyBHbWJIAAALB6Qfbe\
			JyN+9foJhe+/KPFwyU+i++4MAA0q1/WCnizwARRuEPN1aeBQAAASBUZWFtU3BlYWsgc3lzdGV\
			tcyBHbWJIAADrhbI5gUR3thsS7FqKV5P5h7djnwMSJfF2vi58lm1VcwgRUFMAE0P7gAUCGQIA\
			VGVhbVNwZWFrIFN5c3RlbXMgR21iSAAGAwEAAAAFAAf2KhQ7WLjOvwwY0Bi7LxAcWmQeT+LQt\
			uaOzjhYoA+YIBGNq1kRjlQZ").unwrap()).unwrap();
	}

	#[test]
	fn derive_public_key() {
		let licenses = Licenses::parse_ignore_expired(&base64::decode("AQA1hUFJiiSs0wFXkYuPUJVcDa6XCrZTcsvkB0Ffzz4CmwIITRXgCqeTYAcAAAAgQW5vbnltb3VzAAC4R+5mos+UQ/KCbkpQLMI5WRp4wkQu8e5PZY4zU+/FlyAJwaE8CcJJ/A==").unwrap()).unwrap();
		let derived_key =
			licenses.derive_public_key(EccKeyPubEd25519::from_bytes(crate::ROOT_KEY)).unwrap();

		let expected_key = [
			0x40, 0xe9, 0x50, 0xc4, 0x61, 0xba, 0x18, 0x3a, 0x1e, 0xb7, 0xcb, 0xb1, 0x9a, 0xc3,
			0xd8, 0xd9, 0xc4, 0xd5, 0x24, 0xdb, 0x38, 0xf7, 0x2d, 0x3d, 0x66, 0x75, 0x77, 0x2a,
			0xc5, 0x9c, 0xc5, 0xc6,
		];
		let derived_key = derived_key.compress().0;
		//println!("Derived key: {:?}", ::utils::HexSlice((&derived_key) as &[u8]));
		assert_eq!(derived_key, expected_key);
	}
}
