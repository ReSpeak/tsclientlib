use std::str;

use byteorder::{BigEndian, ReadBytesExt};
use chrono::{DateTime, NaiveDateTime, Utc};
use curve25519_dalek::edwards::{CompressedEdwardsY, EdwardsPoint};
use curve25519_dalek::scalar::Scalar;
use num::FromPrimitive;
use ring::digest;

use {Result};
use crypto::EccKeyPubEd25519;

#[derive(Debug)]
pub struct License {
    pub key: EccKeyPubEd25519,
    pub not_valid_before: DateTime<Utc>,
    pub not_valid_after: DateTime<Utc>,
    /// First 32 byte of SHA512(last 4 bytes from key || rest of license block)
    pub hash: [u8; 32],
    pub inner: InnerLicense,
}

#[derive(Debug, FromPrimitive, ToPrimitive)]
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

#[derive(Debug)]
pub enum InnerLicense {
    Intermediate {
        issuer: String,
    }, // 0
    Website {
        issuer: String,
    }, // 1
    Server {
        issuer: String,
        license_type: LicenseType,
    }, // 2
    Code {
        issuer: String,
    }, // 3
    // 4: Token, 5: LicenseSign, 6: MyTsIdSign (not existing in the license
    // parameter)
    Ephemeral, // 32
}

#[derive(Debug)]
pub struct Licenses {
    blocks: Vec<License>,
}

impl Licenses {
    pub fn parse(mut data: &[u8]) -> Result<Self> {
        let version = data[0];
        if version != 1 {
            return Err(format_err!("Unsupported version").into());
        }
        // Read licenses
        let mut res = Licenses { blocks: Vec::new() };
        data = &data[1..];
        while !data.is_empty() {
            // Read next license
            let (license, len) = License::parse(data)?;

            // TODO Check valid times

            res.blocks.push(license);
            data = &data[len..];
        }
        Ok(res)
    }

    pub fn derive_public_key(&self) -> Result<EdwardsPoint> {
        let mut last_round = CompressedEdwardsY(::ROOT_KEY).decompress()
            .unwrap();
        for l in &self.blocks {
            //let derived_key = last_round.compress().0;
            //println!("Got key ({}): {:?}", i, ::utils::HexSlice((&derived_key) as &[u8]));
            last_round = l.derive_public_key(&last_round)?;
        }
        Ok(last_round)
    }
}

impl License {
    /// Parse license and return read length.
    pub fn parse(data: &[u8]) -> Result<(Self, usize)> {
        const MIN_LEN: usize = 42;

        if data.len() < MIN_LEN {
            return Err(format_err!("License too short").into());
        }
        if data[0] != 0 {
            return Err(format_err!("Wrong key kind {} in license", data[0])
                .into());
        }

        let mut key_data = [0; 32];
        key_data.copy_from_slice(&data[1..33]);

        let before_ts = (&data[34..]).read_u32::<BigEndian>()?;
        let after_ts = (&data[38..]).read_u32::<BigEndian>()?;

        let (inner, extra_len) = match data[33] {
            0 => {
                let len = if let Some(len) = data[46..].iter()
                    .position(|&b| b == 0) {
                    len
                } else {
                    return Err(format_err!("Non-null-terminated string")
                        .into());
                };
                let issuer = str::from_utf8(&data[46..46 + len])?.to_string();
                (InnerLicense::Intermediate {
                    issuer,
                }, 5 + len)
            }
            2 => {
                let license_type = LicenseType::from_u8(data[42]).ok_or_else(||
                    format_err!("Unknown license type {}", data[42]))?;
                let len = if let Some(len) = data[47..].iter()
                    .position(|&b| b == 0) {
                    len
                } else {
                    return Err(format_err!("Non-null-terminated string")
                        .into());
                };
                let issuer = str::from_utf8(&data[47..47 + len])?.to_string();
                (InnerLicense::Server {
                    issuer,
                    license_type,
                }, 6 + len)
            }
            32 => (InnerLicense::Ephemeral, 0),
            i => return Err(format_err!("Invalid license block type {}", i)
                .into()),
        };

        let all_len = MIN_LEN + extra_len;
        let hash_out = digest::digest(&digest::SHA512, &data[1..all_len]);
        let mut hash = [0; 32];
        hash.copy_from_slice(&hash_out.as_ref()[..32]);

        Ok((License {
            key: EccKeyPubEd25519::from_bytes(key_data),
            not_valid_before: DateTime::from_utc(NaiveDateTime::from_timestamp(
                i64::from(before_ts) + 0x50e22700, 0), Utc),
            not_valid_after: DateTime::from_utc(NaiveDateTime::from_timestamp(
                i64::from(after_ts) + 0x50e22700, 0), Utc),
            hash,
            inner,
        }, all_len))
    }

    pub fn derive_public_key(&self, parent_key: &EdwardsPoint)
        -> Result<EdwardsPoint> {
        // Make a valid private key from the hash
        let mut priv_key = self.hash.clone();
        priv_key[0] &= 248;
        priv_key[31] &= 63;
        priv_key[31] |= 64;
        let priv_key = Scalar::from_bytes_mod_order(priv_key);
        let pub_key = self.key.0.decompress().ok_or_else(||
            format_err!("Cannot uncompress public key"))?;
        Ok(pub_key * priv_key + parent_key)
    }
}

#[cfg(test)]
mod tests {
    use base64;
    use super::*;

    #[test]
    fn parse_standard_license() {
        Licenses::parse(&base64::decode("AQA1hUFJiiSs0wFXkYuPUJVcDa6XCrZTcsvkB0\
            Ffzz4CmwIITRXgCqeTYAcAAAAgQW5vbnltb3VzAACiIBip9hQaK6P3QhwOJs/BkPn0i\
            oyIDPaNgzJ6M8x0kiAJf4hxCYAxMQ==").unwrap()).unwrap();
    }

    #[test]
    fn parse_aal_license() {
        Licenses::parse(&base64::decode("AQCvbHFTQDY/terPeilrp/ECU9xCH5U3xC92lY\
            TNaY/0KQAJFueAazbsgAAAACVUZWFtU3BlYWsgU3lzdGVtcyBHbWJIAABhl9gwla/UJ\
            p2Eszst9TRVXO/PeE6a6d+CTI6Pg7OEVgAJc5CrL4Nh8gAAACRUZWFtU3BlYWsgc3lz\
            dGVtcyBHbWJIAACvTQIgpv6zmLZq3znh7ygmOSokGFkFjz4bTigrOnetrgIJdIIACdS\
            /gAYAAAAAU29zc2VuU3lzdGVtcy5iaWQAADY7+uV1CQ1niOvYSdGzsu83kPTNWijovr\
            3B78eHGeePIAm98vQJvpu0").unwrap()).unwrap();
    }

    #[test]
    fn derive_public_key() {
        let licenses = Licenses::parse(&base64::decode("AQA1hUFJiiSs0wFXkYuPUJVcDa6XCrZTcsvkB0Ffzz4CmwIITRXgCqeTYAcAAAAgQW5vbnltb3VzAAC4R+5mos+UQ/KCbkpQLMI5WRp4wkQu8e5PZY4zU+/FlyAJwaE8CcJJ/A==").unwrap()).unwrap();
        let derived_key = licenses.derive_public_key().unwrap();

        let expected_key = [0x40, 0xe9, 0x50, 0xc4, 0x61, 0xba, 0x18, 0x3a, 0x1e, 0xb7, 0xcb, 0xb1, 0x9a, 0xc3, 0xd8, 0xd9, 0xc4, 0xd5, 0x24, 0xdb, 0x38, 0xf7, 0x2d, 0x3d, 0x66, 0x75, 0x77, 0x2a, 0xc5, 0x9c, 0xc5, 0xc6];
        let derived_key = derived_key.compress().0;
        //println!("Derived key: {:?}", ::utils::HexSlice((&derived_key) as &[u8]));
        assert_eq!(derived_key, expected_key);
    }
}
