use std::convert::{TryFrom, TryInto};
use std::fmt;
use std::io::Cursor;
use std::str;

use curve25519_dalek_ng::edwards::EdwardsPoint;
use curve25519_dalek_ng::scalar::Scalar;
use num_derive::{FromPrimitive, ToPrimitive};
use num_traits::{FromPrimitive as _, ToPrimitive as _};
use omnom::{ReadExt, WriteExt};
use sha2::{Digest, Sha512};
use thiserror::Error;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
use tracing::trace;
use tsproto_packets::HexSlice;
use tsproto_types::{
	crypto::{EccKeyPrivEd25519, EccKeyPubEd25519},
	LicenseType,
};

pub const TIMESTAMP_OFFSET: i64 = 0x50e2_2700;

const BLOCK_MIN_LEN: usize = 42;
const BLOCK_TYPE_OFFSET: usize = 33;
const BLOCK_NOT_VALID_BEFORE_OFFSET: usize = 34;
const BLOCK_NOT_VALID_AFTER_OFFSET: usize = 38;

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
	#[error("Too many license blocks: {0}")]
	TooManyBlocks(usize),
	#[error("License too short ({0}): {1}")]
	TooShort(usize, &'static str),
	#[error("Unknown license block type {0}")]
	UnknownBlockType(u8),
	#[error("Unknown license type {0}")]
	UnknownLicenseType(u8),
	#[error("Unsupported license version {0}")]
	UnsupportedVersion(u8),
	#[error("Wrong key kind {0} in license")]
	WrongKeyKind(u8),
	#[error("There is no {0} in this block")]
	NoSuchProperty(&'static str),
	#[error("Wrong property type for id {id}: Expected type {expected} but got type {actual}")]
	WrongPropertyType { id: u8, expected: u8, actual: u8 },
	#[error(
		"Propery with id {id} and type {typ} has wrong length: Expected length {expected} but got \
		 length {actual}"
	)]
	WrongPropertyLength { id: u8, typ: u8, expected: u8, actual: u8 },
}

/// A single block of a license
#[derive(Clone, Debug)]
pub struct License {
	/// Length of the block
	pub len: usize,
	/// A private key for the block to derive a private key
	pub private_key: Option<EccKeyPrivEd25519>,
	pub inner: InnerLicense,
}

/// Helper struct to debug print a license block.
pub struct DebugLicense<'a>(&'a License, &'a [u8]);

#[derive(Debug, Clone, Copy, Eq, Hash, PartialEq, FromPrimitive, ToPrimitive)]
pub enum LicenseBlockType {
	Intermediate,
	Website,
	/// Ts3_Server
	Server,
	Code,
	// Not existing in the license parameter:
	// 4: Token, 5: License_Sign, 6: MyTsId_Sign, 7: Updater
	/// Ts_Server_License
	Ts5Server = 8,
	/// 32, Ephemeral_Key
	Ephemeral = 32,
}

#[derive(Debug, Clone)]
pub enum InnerLicense {
	Ts5Server {
		/// Properties from the license, offset from the start of properties for every field
		properties: Vec<usize>,
	},
	Other,
}

/// A list of blocks, forming a complete license
#[derive(Clone)]
pub struct Licenses {
	pub data: Vec<u8>,
	pub blocks: Vec<License>,
}

#[derive(Clone, Debug)]
pub enum LicenseProperty<'a> {
	/// Unknown property with id 1 and type 1
	Unknown1(u32),
	Issuer(&'a str),
	MaxClients(u32),
	Unknown {
		id: u8,
		typ: u8,
		data: &'a [u8],
	},
}

#[derive(Debug)]
pub struct LicenseBuilder<'a> {
	licenses: &'a mut Licenses,
}

#[derive(Debug)]
pub struct LicenseBlockBuilder<'a, 'b> {
	builder: &'b mut LicenseBuilder<'a>,
	/// The start offset for the new block
	offset: usize,
	typ: LicenseBlockType,
}

type LicenseTimeBounds = (OffsetDateTime, OffsetDateTime);

impl Licenses {
	pub fn new() -> Self { Self::default() }

	/// Parse a license but ignore expired licenses.
	///
	/// This is useful for tests but should not be used otherwise.
	pub fn parse_ignore_expired(data: Vec<u8>) -> Result<Self> { Self::parse_internal(data, false) }

	pub fn parse(data: Vec<u8>) -> Result<Self> { Self::parse_internal(data, true) }

	pub fn parse_internal(data: Vec<u8>, check_expired: bool) -> Result<Self> {
		let version = data[0];
		if version != 0 && version != 1 {
			return Err(Error::UnsupportedVersion(version));
		}
		// Read licenses
		let mut blocks = Vec::new();

		let now = OffsetDateTime::now_utc();
		let mut offset = 1;
		let mut bounds = None;
		while data.len() > offset {
			if blocks.len() >= 8 {
				// Accept only 8 blocks
				return Err(Error::TooManyBlocks(blocks.len()));
			}

			// Read next license
			let (license, b) = Self::parse_block(&data[offset..], check_expired, now, bounds)?;
			offset += license.len;
			bounds = Some(b);

			blocks.push(license);
		}

		Ok(Licenses { data, blocks })
	}

	fn parse_block(
		data: &[u8], check_expired: bool, now: OffsetDateTime, bounds: Option<LicenseTimeBounds>,
	) -> Result<(License, LicenseTimeBounds)> {
		let license = License::parse(data)?;
		let license_data = &data[..license.len];

		// Check if the certificate is valid
		let new_bounds = (
			license.get_not_valid_before(license_data)?,
			license.get_not_valid_after(license_data)?,
		);
		if new_bounds.0 > now && check_expired {
			return Err(Error::Expired { start: new_bounds.0, end: new_bounds.1 });
		}
		if new_bounds.1 < now && check_expired {
			return Err(Error::Expired { start: new_bounds.0, end: new_bounds.1 });
		}
		if let Some((start, end)) = bounds {
			// The inner license must not have wider bounds
			if new_bounds.0 < start || new_bounds.1 > end {
				return Err(Error::Bounds {
					outer_start: start,
					outer_end: end,
					inner_start: new_bounds.0,
					inner_end: new_bounds.1,
				});
			}
		}
		Ok((license, new_bounds))
	}

	pub fn derive_public_key(&self, root: EccKeyPubEd25519) -> Result<EdwardsPoint> {
		let mut last_round = root.0.decompress().ok_or(Error::InvalidRootKey)?;
		let mut offset = 1;
		trace!("Deriving public key for license");
		for l in &self.blocks {
			trace!(current_key = %HexSlice((&last_round.compress().0) as &[u8]));
			last_round = l.derive_public_key(&self.data[offset..offset + l.len], &last_round)?;
			offset += l.len;
		}
		trace!(
			final_key = %HexSlice((&last_round.compress().0) as &[u8]),
			"Finished deriving public key"
		);
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
		let mut offset = 1;
		for (i, l) in self.blocks.iter().enumerate() {
			if i >= starting_block {
				res = l.derive_private_key(&self.data[offset..offset + l.len], &res)?;
			}
			offset += l.len;
		}
		Ok(res)
	}

	pub fn build<F: FnOnce(LicenseBuilder)>(mut self, f: F) -> Result<Self> {
		let len = self.data.len();

		f(LicenseBuilder { licenses: &mut self });

		// Parse new blocks
		let now = OffsetDateTime::now_utc();
		let mut offset = len;
		let mut bounds = self
			.blocks
			.last()
			.map(|b| {
				let b_offset = len - b.len;
				let b_data = &self.data[b_offset..len];
				Ok((b.get_not_valid_before(b_data)?, b.get_not_valid_after(b_data)?))
			})
			.transpose()?;

		while self.data.len() > offset {
			if self.blocks.len() >= 8 {
				// Accept only 8 blocks
				return Err(Error::TooManyBlocks(self.blocks.len()));
			}

			// Read next license
			let (license, b) = Self::parse_block(&self.data[offset..], false, now, bounds)?;
			offset += license.len;
			bounds = Some(b);

			self.blocks.push(license);
		}

		Ok(self)
	}

	pub fn is_valid(&self) -> Result<()> {
		let mut offset = 1;
		for b in &self.blocks {
			b.is_valid(&self.data[offset..offset + b.len])?;
			offset += b.len;
		}
		Ok(())
	}
}

impl Default for Licenses {
	fn default() -> Self { Self { data: vec![1], blocks: Default::default() } }
}

impl fmt::Debug for Licenses {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "Licenses ")?;
		let mut offset = 1;
		f.debug_list()
			.entries(self.blocks.iter().map(|b| {
				let o = offset;
				offset += b.len;
				DebugLicense(b, &self.data[o..o + b.len])
			}))
			.finish()?;
		Ok(())
	}
}

impl License {
	/// Parse license and return read length.
	pub fn parse(data: &[u8]) -> Result<Self> {
		let len = data.len();
		if data.len() < BLOCK_MIN_LEN {
			return Err(Error::TooShort(len, "less than minimum block length"));
		}
		if data[0] != 0 {
			return Err(Error::WrongKeyKind(data[0]));
		}

		let typ_i = data[BLOCK_TYPE_OFFSET];
		let typ = LicenseBlockType::from_u8(typ_i).ok_or(Error::UnknownBlockType(typ_i))?;
		let data = &data[BLOCK_MIN_LEN..];
		let (inner, extra_len) = match typ {
			LicenseBlockType::Intermediate => {
				if data.len() < 5 {
					return Err(Error::TooShort(
						len,
						"less than minimum length of intermediate license",
					));
				}
				// 4 byte unknown
				// Issuer string

				let len = if let Some(len) = data[4..].iter().position(|&b| b == 0) {
					len
				} else {
					return Err(Error::NonterminatedString);
				};
				(InnerLicense::Other, 5 + len)
			}
			LicenseBlockType::Server => {
				if data.len() < 6 {
					return Err(Error::TooShort(
						len,
						"less than minimum length of TS3 server license",
					));
				}
				// 1 byte server license type
				// 4 byte max clients
				// Issuer string
				let len = if let Some(len) = data[5..].iter().position(|&b| b == 0) {
					len
				} else {
					return Err(Error::NonterminatedString);
				};

				(InnerLicense::Other, 6 + len)
			}
			LicenseBlockType::Ts5Server => {
				if data.len() < 2 {
					return Err(Error::TooShort(
						len,
						"less than minimum length of TS5 server license",
					));
				}
				// 1 byte server license type
				// Property count and properties
				let property_count = data[1];
				let mut pos = 2;

				let properties = (0..property_count)
					.map(|_| {
						let p = pos - 2;
						if pos >= data.len() {
							return Err(Error::TooShort(len, "missing TS5 license property"));
						}
						let len = usize::from(data[pos]);
						pos += 1;
						if pos + len >= data.len() {
							return Err(Error::TooShort(len, "cut off TS5 license property"));
						}
						pos += len;
						Ok(p)
					})
					.collect::<Result<_>>()?;

				(InnerLicense::Ts5Server { properties }, pos)
			}
			LicenseBlockType::Ephemeral => (InnerLicense::Other, 0),
			_ => {
				return Err(Error::UnknownBlockType(typ_i));
			}
		};

		Ok(License { len: BLOCK_MIN_LEN + extra_len, private_key: None, inner })
	}

	pub fn get_type(&self, data: &[u8]) -> Result<LicenseBlockType> {
		let typ_i = data[BLOCK_TYPE_OFFSET];
		LicenseBlockType::from_u8(typ_i).ok_or(Error::UnknownBlockType(typ_i))
	}

	pub fn get_public_key(&self, data: &[u8]) -> Result<EdwardsPoint> {
		let k = EccKeyPubEd25519::from_bytes(data[1..33].try_into().unwrap());
		k.0.decompress().ok_or(Error::InvalidPublicKey)
	}

	fn get_timestamp(&self, data: &[u8], block_offset: usize) -> Result<OffsetDateTime> {
		let num: u32 = (&data[block_offset..]).read_be().map_err(Error::Deserialize)?;
		OffsetDateTime::from_unix_timestamp(i64::from(num) + TIMESTAMP_OFFSET)
			.map_err(Error::DeserializeDate)
	}

	pub fn get_not_valid_before(&self, data: &[u8]) -> Result<OffsetDateTime> {
		self.get_timestamp(data, BLOCK_NOT_VALID_BEFORE_OFFSET)
	}

	pub fn get_not_valid_after(&self, data: &[u8]) -> Result<OffsetDateTime> {
		self.get_timestamp(data, BLOCK_NOT_VALID_AFTER_OFFSET)
	}

	pub fn get_license_type(&self, data: &[u8]) -> Result<LicenseType> {
		if ![LicenseBlockType::Server, LicenseBlockType::Ts5Server].contains(&self.get_type(data)?)
		{
			return Err(Error::NoSuchProperty("license type"));
		}
		LicenseType::from_u8(data[BLOCK_MIN_LEN])
			.ok_or(Error::UnknownLicenseType(data[BLOCK_MIN_LEN]))
	}

	pub fn get_issuer<'a>(&self, data: &'a [u8]) -> Result<&'a str> {
		let typ = self.get_type(data)?;

		if typ == LicenseBlockType::Ts5Server {
			// Search in properties
			let res = self
				.get_properties(data)?
				.find_map(|p| if let Ok(LicenseProperty::Issuer(i)) = p { Some(i) } else { None })
				.ok_or(Error::NoSuchProperty("issuer"));
			return res;
		}

		let offset = BLOCK_MIN_LEN
			+ match typ {
				LicenseBlockType::Intermediate => 4,
				LicenseBlockType::Website | LicenseBlockType::Code => 0,
				LicenseBlockType::Server => 5,
				_ => return Err(Error::NoSuchProperty("issuer")),
			};
		let len = match typ {
			LicenseBlockType::Intermediate
			| LicenseBlockType::Website
			| LicenseBlockType::Code
			| LicenseBlockType::Server => self.len - offset - 1,
			_ => return Err(Error::NoSuchProperty("issuer")),
		};
		str::from_utf8(&data[offset..offset + len]).map_err(Error::DeserializeString)
	}

	pub fn get_max_clients(&self, data: &[u8]) -> Result<u32> {
		let typ = self.get_type(data)?;

		if typ == LicenseBlockType::Ts5Server {
			// Search in properties
			let res = self
				.get_properties(data)?
				.find_map(
					|p| if let Ok(LicenseProperty::MaxClients(i)) = p { Some(i) } else { None },
				)
				.ok_or(Error::NoSuchProperty("max clients"));
			return res;
		}

		let offset = BLOCK_MIN_LEN
			+ match typ {
				LicenseBlockType::Intermediate => 0,
				LicenseBlockType::Server => 1,
				_ => return Err(Error::NoSuchProperty("max clients")),
			};
		(&data[offset..offset + 4]).read_be().map_err(Error::Deserialize)
	}

	pub fn get_properties<'a: 'b, 'b>(
		&'b self, data: &'a [u8],
	) -> Result<impl Iterator<Item = Result<LicenseProperty<'a>>> + 'b> {
		if let InnerLicense::Ts5Server { properties } = &self.inner {
			Ok(properties.iter().map(move |p| {
				let o = BLOCK_MIN_LEN + 2 + p;
				let len = data[o];
				let len_usize = len as usize;
				let id = data[o + 1];
				let typ = data[o + 2];

				// Check length
				if let Some(expected) = match typ {
					// String, check that it is null-terminated
					0 => {
						if data[o + len_usize] != 0 {
							return Err(Error::NonterminatedString);
						}
						None
					}
					1 | 3 => Some(4),
					2 | 4 => Some(8),
					_ => None,
				} {
					if len != expected {
						return Err(Error::WrongPropertyLength { id, typ, expected, actual: len });
					}
				}

				match id {
					1 => {
						if typ != 1 {
							return Err(Error::WrongPropertyType { id, expected: 1, actual: typ });
						}
						Ok(LicenseProperty::Unknown1(
							(&data[o + 3..o + 3 + 4]).read_be().map_err(Error::Deserialize)?,
						))
					}
					2 => {
						if typ != 0 {
							return Err(Error::WrongPropertyType { id, expected: 0, actual: typ });
						}
						let s = str::from_utf8(&data[o + 3..o + len_usize])
							.map_err(Error::DeserializeString)?;
						Ok(LicenseProperty::Issuer(s))
					}
					3 => {
						if typ != 1 {
							return Err(Error::WrongPropertyType { id, expected: 1, actual: typ });
						}
						Ok(LicenseProperty::MaxClients(
							(&data[o + 3..o + 3 + 4]).read_be().map_err(Error::Deserialize)?,
						))
					}
					_ => {
						Ok(LicenseProperty::Unknown { id, typ, data: &data[o + 3..o + len_usize] })
					}
				}
			}))
		} else {
			Err(Error::NoSuchProperty("properties"))
		}
	}

	/// Get the private key from the hash of this license block.
	pub fn get_hash_key(&self, data: &[u8]) -> Scalar {
		// Make a valid private key from the hash
		let mut hash_key = Sha512::digest(&data[1..]);
		hash_key[0] &= 248;
		hash_key[31] &= 63;
		hash_key[31] |= 64;
		Scalar::from_bytes_mod_order(hash_key.as_slice()[..32].try_into().unwrap())
	}

	pub fn derive_public_key(
		&self, data: &[u8], parent_key: &EdwardsPoint,
	) -> Result<EdwardsPoint> {
		let pub_key = self.get_public_key(data)?;
		let hash_key = self.get_hash_key(data);
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
		&self, data: &[u8], parent_key: &EccKeyPrivEd25519,
	) -> Result<EccKeyPrivEd25519> {
		let priv_key = if let Some(k) = &self.private_key {
			&k.0
		} else {
			return Err(Error::NoPrivateKey);
		};
		let hash_key = self.get_hash_key(data);
		Ok(EccKeyPrivEd25519(priv_key * hash_key + parent_key.0))
	}

	/// Check if this is a valid license.
	///
	/// Does some sanity checking on fields. Does not need to be called for correctness.
	pub fn is_valid(&self, data: &[u8]) -> Result<()> {
		self.get_public_key(data)?;
		self.get_not_valid_before(data)?;
		self.get_not_valid_after(data)?;

		if let Err(e) = self.get_license_type(data) {
			if !matches!(e, Error::NoSuchProperty(_)) {
				return Err(e);
			}
		}

		if let Err(e) = self.get_issuer(data) {
			if !matches!(e, Error::NoSuchProperty(_)) {
				return Err(e);
			}
		}

		match self.get_properties(data) {
			Ok(ps) => {
				for p in ps {
					p?;
				}
			}
			Err(e) => {
				if !matches!(e, Error::NoSuchProperty(_)) {
					return Err(e);
				}
			}
		}

		Ok(())
	}
}

impl fmt::Debug for DebugLicense<'_> {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		let from = self
			.0
			.get_not_valid_before(self.1)
			.ok()
			.and_then(|d| d.format(&Rfc3339).ok())
			.unwrap_or_else(|| "error".to_string());
		let to = self
			.0
			.get_not_valid_after(self.1)
			.ok()
			.and_then(|d| d.format(&Rfc3339).ok())
			.unwrap_or_else(|| "error".to_string());

		let mut d = f.debug_struct("License");
		let typ = self.0.get_type(self.1).unwrap();
		d.field("type", &typ);
		if let Some(key) = &self.0.private_key {
			d.field("private_key", key);
		}
		d.field("not_valid_before", &from).field("not_valid_after", &to);

		match typ {
			LicenseBlockType::Intermediate => {
				if let Ok(num) = self.0.get_max_clients(self.1) {
					d.field("data", &num);
				} else {
					d.field("data", &"error");
				}
			}
			LicenseBlockType::Website | LicenseBlockType::Code => {
				if let Ok(issuer) = self.0.get_issuer(self.1) {
					d.field("issuer", &issuer);
				} else {
					d.field("issuer", &"error");
				}
			}
			LicenseBlockType::Server => {
				if let Ok(license_type) = self.0.get_license_type(self.1) {
					d.field("license_type", &license_type);
				} else {
					d.field("license_type", &"error");
				}
				if let Ok(issuer) = self.0.get_issuer(self.1) {
					d.field("issuer", &issuer);
				} else {
					d.field("issuer", &"error");
				}
				if let Ok(max_clients) = self.0.get_max_clients(self.1) {
					d.field("max_clients", &max_clients);
				} else {
					d.field("max_clients", &"error");
				}
			}
			LicenseBlockType::Ts5Server => {
				if let Ok(props) =
					self.0.get_properties(self.1).and_then(|ps| ps.collect::<Result<Vec<_>>>())
				{
					d.field("properties", &props);
				} else {
					d.field("properties", &"error");
				}
			}
			LicenseBlockType::Ephemeral => {}
		}

		d.finish()?;

		Ok(())
	}
}

impl LicenseBlockType {
	fn min_len(&self) -> usize {
		BLOCK_MIN_LEN
			+ match self {
				Self::Intermediate => 5,
				Self::Website | Self::Code => 1,
				Self::Server => 6,
				Self::Ts5Server => 2,
				Self::Ephemeral => 0,
			}
	}
}

impl<'a> LicenseBuilder<'a> {
	pub fn add_block<'b>(&'b mut self, typ: LicenseBlockType) -> LicenseBlockBuilder<'a, 'b> {
		let len = self.licenses.data.len();
		self.licenses.data.resize(len + typ.min_len(), 0);
		self.licenses.data[len + BLOCK_TYPE_OFFSET] = typ.to_u8().unwrap();
		LicenseBlockBuilder { builder: self, offset: len, typ }
	}
}

impl LicenseBlockBuilder<'_, '_> {
	pub fn public_key(&mut self, key: &EccKeyPubEd25519) -> &mut Self {
		let o = self.offset + 1;
		self.builder.licenses.data[o..o + 32].copy_from_slice(&key.0.0);
		self
	}

	fn write_timestamp(&mut self, offset: usize, time: OffsetDateTime) -> &mut Self {
		let o = self.offset + offset;
		Cursor::new(&mut self.builder.licenses.data[o..o + 4])
			.write_be((time.unix_timestamp() - TIMESTAMP_OFFSET) as u32)
			.unwrap();
		self
	}

	pub fn not_valid_before(&mut self, time: OffsetDateTime) -> &mut Self {
		self.write_timestamp(BLOCK_NOT_VALID_BEFORE_OFFSET, time)
	}

	pub fn not_valid_after(&mut self, time: OffsetDateTime) -> &mut Self {
		self.write_timestamp(BLOCK_NOT_VALID_AFTER_OFFSET, time)
	}

	/// Only valid for Server and Ts5Server license types
	pub fn server_license_type(&mut self, typ: LicenseType) -> &mut Self {
		if ![LicenseBlockType::Server, LicenseBlockType::Ts5Server].contains(&self.typ) {
			panic!(
				"Setting the license type is only allowed for Server and Ts5Server license types, \
				 but this is a {:?}",
				self.typ
			);
		}
		self.builder.licenses.data[self.offset + BLOCK_MIN_LEN] = typ.to_u8().unwrap();
		self
	}

	/// Add a property in a TS5 license block.
	///
	/// `len` is the length of the content, excluding id, type and length field.
	/// Return the byte offset to where the contents of the propery starts.
	fn ts5_add_property(&mut self, id: u8, typ: u8, len: usize) -> usize {
		let o = self.builder.licenses.data.len();
		self.builder.licenses.data.resize(o + len + 3, 0);
		// Increment property count
		self.builder.licenses.data[self.offset + BLOCK_MIN_LEN + 1] += 1;
		self.builder.licenses.data[o] = u8::try_from(len).unwrap() + 2;
		self.builder.licenses.data[o + 1] = id;
		self.builder.licenses.data[o + 2] = typ;
		o + 3
	}

	/// Only valid for Intermediate, Server and Ts5Server license types
	pub fn max_clients(&mut self, max_clients: u32) -> &mut Self {
		let o = if self.typ == LicenseBlockType::Ts5Server {
			self.ts5_add_property(3, 1, 4)
		} else {
			self.offset
				+ BLOCK_MIN_LEN
				+ match self.typ {
					LicenseBlockType::Intermediate => 0,
					LicenseBlockType::Server => 1,
					_ => panic!(
						"Setting max clients is only allowed for Intermediate, Server and \
						 Ts5Server license types, but this is a {:?}",
						self.typ
					),
				}
		};
		Cursor::new(&mut self.builder.licenses.data[o..o + 4]).write_be(max_clients).unwrap();
		self
	}

	/// Only valid for Intermediate, Server and Ts5Server license types
	pub fn issuer(&mut self, issuer: &str) -> &mut Self {
		// Check that the issuer does not contain null bytes
		assert!(
			!issuer.contains('\0'),
			"The issuer is written as a null-terminated string so it cannot contain null bytes"
		);

		let o = if self.typ == LicenseBlockType::Ts5Server {
			self.ts5_add_property(2, 0, issuer.len() + 1)
		} else {
			if ![LicenseBlockType::Intermediate, LicenseBlockType::Server].contains(&self.typ) {
				panic!(
					"Setting the issuer is only allowed for Intermediate, Server and Ts5Server \
					 license types, but this is a {:?}",
					self.typ
				);
			}
			// Check that no issuer was written so far
			assert!(
				self.builder.licenses.data.len() - self.offset == self.typ.min_len(),
				"Only a single issuer can be written into a license block"
			);
			let o = self.builder.licenses.data.len();
			self.builder.licenses.data.resize(o + issuer.len(), 0);
			o - 1
		};
		self.builder.licenses.data[o..o + issuer.len()].copy_from_slice(issuer.as_bytes());
		self
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use base64::prelude::*;

	#[test]
	fn parse_standard_license() {
		Licenses::parse_ignore_expired(BASE64_STANDARD.decode("AQA1hUFJiiSs0wFXkYuPUJVcDa6XCrZTcsvkB0\
			Ffzz4CmwIITRXgCqeTYAcAAAAgQW5vbnltb3VzAACiIBip9hQaK6P3QhwOJs/BkPn0i\
			oyIDPaNgzJ6M8x0kiAJf4hxCYAxMQ==").unwrap()).unwrap();
	}

	#[test]
	fn parse_standard_license_expired() {
		assert!(Licenses::parse(BASE64_STANDARD.decode("AQA1hUFJiiSs0wFXkYuPUJVcDa6XCrZTcsvkB0\
			Ffzz4CmwIITRXgCqeTYAcAAAAgQW5vbnltb3VzAACiIBip9hQaK6P3QhwOJs/BkPn0i\
			oyIDPaNgzJ6M8x0kiAJf4hxCYAxMQ==").unwrap()).is_err());
	}

	#[test]
	fn parse_aal_license() {
		Licenses::parse_ignore_expired(BASE64_STANDARD.decode("AQCvbHFTQDY/terPeilrp/ECU9xCH5U3xC92lY\
			TNaY/0KQAJFueAazbsgAAAACVUZWFtU3BlYWsgU3lzdGVtcyBHbWJIAABhl9gwla/UJ\
			p2Eszst9TRVXO/PeE6a6d+CTI6Pg7OEVgAJc5CrL4Nh8gAAACRUZWFtU3BlYWsgc3lz\
			dGVtcyBHbWJIAACvTQIgpv6zmLZq3znh7ygmOSokGFkFjz4bTigrOnetrgIJdIIACdS\
			/gAYAAAAAU29zc2VuU3lzdGVtcy5iaWQAADY7+uV1CQ1niOvYSdGzsu83kPTNWijovr\
			3B78eHGeePIAm98vQJvpu0").unwrap()).unwrap();
	}

	#[test]
	fn parse_ts5_server_license_long() {
		// Slightly older format having a block with a Ts5 license block in it
		Licenses::parse_ignore_expired(BASE64_STANDARD.decode("AQDVsMGbcrMmGif1vSXPWWXNW2CB5\
			Fe9oZ/2uxP29j1EXQAQSfiAazbsgAAAASVUZWFtU3BlYWsgU3lzdGVtcyBHbWJIAAALB6Qfbe\
			JyN+9foJhe+/KPFwyU+i++4MAA0q1/WCnizwARRuEPN1aeBQAAASBUZWFtU3BlYWsgc3lzdGV\
			tcyBHbWJIAADrhbI5gUR3thsS7FqKV5P5h7djnwMSJfF2vi58lm1VcwgRUFMAE0P7gAUCGQIA\
			VGVhbVNwZWFrIFN5c3RlbXMgR21iSAAGAwEAAAAFAAf2KhQ7WLjOvwwY0Bi7LxAcWmQeT+LQt\
			uaOzjhYoA+YIBGNq1kRjlQZ").unwrap()).unwrap();
	}

	#[test]
	fn parse_ts5_server_license_long2() {
		// Also slightly older, has 3 properties
		Licenses::parse_ignore_expired(BASE64_STANDARD.decode("AQDVsMGbcrMmGif1vSXPWWXNW2CB5\
			Fe9oZ/2uxP29j1EXQAQSfiAazbsgAAAASVUZWFtU3BlYWsgU3lzdGVtcyBHbWJIAAAtXG5p2n\
			iXlDfpVAGuD88w8hetKYL4vqHRkB5xB8ASRwAR2t/MN+ttjAAAASBUZWFtU3BlYWsgc3lzdGV\
			tcyBHbWJIAAAdZYGtwkeZFhzqnoV1uk+Tcphe8GgcqiPVtELF9y4wOAgR4qmAF4jnAAkDGQIA\
			VGVhbVNwZWFrIFN5c3RlbXMgR21iSAAGAwEAAAAFBgEBAAGGoADzyFvD+9G6uhIxmh0jK+Uo8\
			z8fYGJVH81vWFULDS0l8yATKe4cEyqW3A==").unwrap()).unwrap();
	}

	#[test]
	fn parse_ts5_server_license_single_block_with_issuer() {
		Licenses::parse_ignore_expired(
			BASE64_STANDARD
				.decode(
					"AQBgjAAqtcBUrw5futTtkl3+EM3OW4Lal6OTPlwuv4xV/\
					 gIRFlEAG0NlAAcAAAAgQW5vbnltb3VzAACKNY+/\
					 9qCbonCSxG18vBb7y7zPIgDdjTmcZoAHHclnJSATPa69Ez5XfQ==",
				)
				.unwrap(),
		)
		.unwrap();
	}

	#[test]
	fn parse_ts5_server_license_single_block() {
		// Only a single server block and the ephemeral block
		Licenses::parse_ignore_expired(BASE64_STANDARD.decode("AQAuio9ZxThXKE+hmzQyzBRedysp9\
			79JBTv2xP3s2oCkiAgQI70AE+YkAAcBBgMBAAAABQBoazM313063zaipPTH06zrXc91ch3huB\
			YrUET9sEbz1CATKgK8EyqrfA==").unwrap()).unwrap();
	}

	#[test]
	fn derive_public_key() {
		let licenses = Licenses::parse_ignore_expired(BASE64_STANDARD.decode("AQA1hUFJiiSs0wFXkYuPUJVcDa6XCrZTcsvkB0Ffzz4CmwIITRXgCqeTYAcAAAAgQW5vbnltb3VzAAC4R+5mos+UQ/KCbkpQLMI5WRp4wkQu8e5PZY4zU+/FlyAJwaE8CcJJ/A==").unwrap()).unwrap();
		let derived_key =
			licenses.derive_public_key(EccKeyPubEd25519::from_bytes(crate::ROOT_KEY)).unwrap();

		let expected_key = [
			0x40, 0xe9, 0x50, 0xc4, 0x61, 0xba, 0x18, 0x3a, 0x1e, 0xb7, 0xcb, 0xb1, 0x9a, 0xc3,
			0xd8, 0xd9, 0xc4, 0xd5, 0x24, 0xdb, 0x38, 0xf7, 0x2d, 0x3d, 0x66, 0x75, 0x77, 0x2a,
			0xc5, 0x9c, 0xc5, 0xc6,
		];
		let derived_key = derived_key.compress().0;
		assert_eq!(derived_key, expected_key);
	}
}
