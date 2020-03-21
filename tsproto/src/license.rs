use std::borrow::Cow;
use std::io::prelude::*;
use std::str;

use anyhow::format_err;
use curve25519_dalek::constants;
use curve25519_dalek::edwards::{CompressedEdwardsY, EdwardsPoint};
use curve25519_dalek::scalar::Scalar;
use num_derive::{FromPrimitive, ToPrimitive};
use num_traits::{FromPrimitive as _, ToPrimitive as _};
use omnom::{ReadExt, WriteExt};
use ring::digest;
use time::OffsetDateTime;

use crate::crypto::{EccKeyPrivEd25519, EccKeyPubEd25519};
use crate::Result;

pub const TIMESTAMP_OFFSET: i64 = 0x50e2_2700;

/// Either a public or a private key.
#[derive(Debug, Clone)]
pub enum LicenseKey {
	Public(EccKeyPubEd25519),
	Private(EccKeyPrivEd25519),
}

#[derive(Debug, Clone)]
pub struct License {
	pub key: LicenseKey,
	pub not_valid_before: OffsetDateTime,
	pub not_valid_after: OffsetDateTime,
	/// First 32 byte of SHA512(last 4 bytes from key || rest of license block)
	pub hash: [u8; 32],
	pub inner: InnerLicense,
}

#[derive(Debug, Clone, Copy, Hash, FromPrimitive, ToPrimitive)]
pub enum LicenseType {
	None,
	Offline,
	Sdk,
	SdkOffline,
	/// Non-profit license
	Npl,
	/// Authorized Teamspeak hosting provider
	Athp,
	/// Annual activation license
	Aal,
	/// 32 slots default license
	Default,
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
			LicenseKey::Public(ref k) => k.0.decompress().ok_or_else(|| {
				format_err!("Cannot uncompress public key").into()
			}),
			LicenseKey::Private(ref k) => {
				Ok(&constants::ED25519_BASEPOINT_TABLE * &k.0)
			}
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
	pub fn parse_ignore_expired(data: &[u8]) -> Result<Self> {
		Self::parse_internal(data, false)
	}

	pub fn parse(data: &[u8]) -> Result<Self> {
		Self::parse_internal(data, true)
	}

	fn parse_internal(mut data: &[u8], check_expired: bool) -> Result<Self> {
		let version = data[0];
		if version != 1 {
			return Err(format_err!("Unsupported version").into());
		}
		// Read licenses
		let mut res = Licenses { blocks: Vec::new() };
		data = &data[1..];

		let now = OffsetDateTime::now();
		let mut bounds = None;
		while !data.is_empty() {
			if res.blocks.len() >= 8 {
				// Accept only 8 blocks
				return Err(format_err!("Too many license blocks").into());
			}

			// Read next license
			let (license, len) = License::parse(data)?;

			// Check if the certificate is valid
			if license.not_valid_before > now && check_expired {
				return Err(format_err!("License is not yet valid").into());
			}
			if license.not_valid_after < now && check_expired {
				return Err(format_err!("License expired").into());
			}
			if let Some((start, end)) = bounds {
				// The inner license must not have wider bounds
				if license.not_valid_before < start
					|| license.not_valid_after > end
				{
					return Err(
						format_err!("Invalid license time bounds").into()
					);
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
		w.write_be(1u8)?;

		for l in &self.blocks {
			l.write(w)?;
		}

		Ok(())
	}

	pub fn derive_public_key(&self) -> Result<EdwardsPoint> {
		let mut last_round =
			CompressedEdwardsY(crate::ROOT_KEY).decompress().unwrap();
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
		let hash_out = digest::digest(&digest::SHA512, &data[1..]);
		let mut hash = [0; 32];
		hash.copy_from_slice(&hash_out.as_ref()[..32]);
		self.hash = hash;
	}

	/// Parse license and return read length.
	pub fn parse(data: &[u8]) -> Result<(Self, usize)> {
		const MIN_LEN: usize = 42;

		if data.len() < MIN_LEN {
			return Err(format_err!("License too short").into());
		}
		if data[0] != 0 {
			return Err(
				format_err!("Wrong key kind {} in license", data[0]).into()
			);
		}

		let mut key_data = [0; 32];
		key_data.copy_from_slice(&data[1..33]);

		let before_ts: u32 = (&data[34..]).read_be()?;
		let after_ts: u32 = (&data[38..]).read_be()?;

		let (inner, extra_len) = match data[33] {
			0 => {
				let license_data: u32 = (&data[42..]).read_be()?;
				if license_data > 0x7f {
					return Err(format_err!(
						"Invalid data in intermediate license"
					)
					.into());
				}

				let len = if let Some(len) =
					data[46..].iter().position(|&b| b == 0)
				{
					len
				} else {
					return Err(
						format_err!("Non-null-terminated string").into()
					);
				};
				let issuer = str::from_utf8(&data[46..46 + len])?.to_string();
				(
					InnerLicense::Intermediate {
						issuer,
						data: license_data as u8,
					},
					5 + len,
				)
			}
			2 => {
				if data.len() < 47 {
					return Err(format_err!("License too short").into());
				}
				let license_type =
					LicenseType::from_u8(data[42]).ok_or_else(|| {
						format_err!("Unknown license type {}", data[42])
					})?;
				let license_data: u32 = (&data[43..]).read_be()?;
				let len = if let Some(len) =
					data[47..].iter().position(|&b| b == 0)
				{
					len
				} else {
					return Err(
						format_err!("Non-null-terminated string").into()
					);
				};
				if data.len() < 47 + len {
					return Err(format_err!("License too short").into());
				}
				let issuer = str::from_utf8(&data[47..47 + len])?.to_string();
				(
					InnerLicense::Server {
						issuer,
						license_type,
						data: license_data,
					},
					6 + len,
				)
			}
			32 => (InnerLicense::Ephemeral, 0),
			i => {
				return Err(
					format_err!("Invalid license block type {}", i).into()
				);
			}
		};

		let all_len = MIN_LEN + extra_len;
		let hash_out = digest::digest(&digest::SHA512, &data[1..all_len]);
		let mut hash = [0; 32];
		hash.copy_from_slice(&hash_out.as_ref()[..32]);

		Ok((
			License {
				key: LicenseKey::Public(EccKeyPubEd25519::from_bytes(key_data)),
				not_valid_before: OffsetDateTime::from_unix_timestamp(
					i64::from(before_ts) + TIMESTAMP_OFFSET,
				),
				not_valid_after: OffsetDateTime::from_unix_timestamp(
					i64::from(after_ts) + TIMESTAMP_OFFSET,
				),
				hash,
				inner,
			},
			all_len,
		))
	}

	pub fn write(&self, mut w: &mut dyn Write) -> Result<()> {
		w.write_be(0u8)?;
		// Public key
		w.write_all(self.key.get_pub_bytes().as_ref())?;
		// Type
		w.write_be(self.inner.type_id())?;

		w.write_be(
			(self.not_valid_before.timestamp() - TIMESTAMP_OFFSET) as u32,
		)?;
		w.write_be(
			(self.not_valid_after.timestamp() - TIMESTAMP_OFFSET) as u32,
		)?;

		match self.inner {
			InnerLicense::Intermediate { ref issuer, data } => {
				w.write_be(u32::from(data))?;
				w.write_all(issuer.as_bytes())?;
				w.write_be(0u8)?;
			}
			InnerLicense::Website { ref issuer } => {
				w.write_all(issuer.as_bytes())?;
				w.write_be(0u8)?;
			}
			InnerLicense::Server { ref issuer, license_type, data } => {
				w.write_be(license_type.to_u8().unwrap())?;
				w.write_be(data)?;
				w.write_all(issuer.as_bytes())?;
				w.write_be(0u8)?;
			}
			InnerLicense::Code { ref issuer } => {
				w.write_all(issuer.as_bytes())?;
				w.write_be(0u8)?;
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

	pub fn derive_public_key(
		&self, parent_key: &EdwardsPoint,
	) -> Result<EdwardsPoint> {
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
	pub fn derive_private_key(
		&self, parent_key: &EccKeyPrivEd25519,
	) -> Result<EccKeyPrivEd25519> {
		let priv_key = if let LicenseKey::Private(ref k) = self.key {
			&k.0
		} else {
			return Err(format_err!("License contains no private key").into());
		};
		let hash_key = self.get_hash_key();
		Ok(EccKeyPrivEd25519(priv_key * hash_key + parent_key.0))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use base64;

	#[test]
	#[should_panic]
	fn parse_standard_license() {
		Licenses::parse(&base64::decode("AQA1hUFJiiSs0wFXkYuPUJVcDa6XCrZTcsvkB0\
			Ffzz4CmwIITRXgCqeTYAcAAAAgQW5vbnltb3VzAACiIBip9hQaK6P3QhwOJs/BkPn0i\
			oyIDPaNgzJ6M8x0kiAJf4hxCYAxMQ==").unwrap()).unwrap();
	}

	#[test]
	#[should_panic]
	fn parse_aal_license() {
		Licenses::parse(&base64::decode("AQCvbHFTQDY/terPeilrp/ECU9xCH5U3xC92lY\
			TNaY/0KQAJFueAazbsgAAAACVUZWFtU3BlYWsgU3lzdGVtcyBHbWJIAABhl9gwla/UJ\
			p2Eszst9TRVXO/PeE6a6d+CTI6Pg7OEVgAJc5CrL4Nh8gAAACRUZWFtU3BlYWsgc3lz\
			dGVtcyBHbWJIAACvTQIgpv6zmLZq3znh7ygmOSokGFkFjz4bTigrOnetrgIJdIIACdS\
			/gAYAAAAAU29zc2VuU3lzdGVtcy5iaWQAADY7+uV1CQ1niOvYSdGzsu83kPTNWijovr\
			3B78eHGeePIAm98vQJvpu0").unwrap()).unwrap();
	}

	#[test]
	#[should_panic]
	fn derive_public_key() {
		let licenses = Licenses::parse(&base64::decode("AQA1hUFJiiSs0wFXkYuPUJVcDa6XCrZTcsvkB0Ffzz4CmwIITRXgCqeTYAcAAAAgQW5vbnltb3VzAAC4R+5mos+UQ/KCbkpQLMI5WRp4wkQu8e5PZY4zU+/FlyAJwaE8CcJJ/A==").unwrap()).unwrap();
		let derived_key = licenses.derive_public_key().unwrap();

		let expected_key = [
			0x40, 0xe9, 0x50, 0xc4, 0x61, 0xba, 0x18, 0x3a, 0x1e, 0xb7, 0xcb,
			0xb1, 0x9a, 0xc3, 0xd8, 0xd9, 0xc4, 0xd5, 0x24, 0xdb, 0x38, 0xf7,
			0x2d, 0x3d, 0x66, 0x75, 0x77, 0x2a, 0xc5, 0x9c, 0xc5, 0xc6,
		];
		let derived_key = derived_key.compress().0;
		//println!("Derived key: {:?}", ::utils::HexSlice((&derived_key) as &[u8]));
		assert_eq!(derived_key, expected_key);
	}
}
