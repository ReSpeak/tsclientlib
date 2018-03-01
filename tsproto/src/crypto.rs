//! This module contains cryptography related code.
use std::fmt;

use base64;
use num::BigUint;

use openssl::bn::BigNumContext;
use openssl::derive::Deriver;
use openssl::ec::{self, EcGroup, EcKey, EcPoint};
use openssl::hash::MessageDigest;
use openssl::nid::Nid;
use openssl::pkey::{self, PKey, Private, Public};
use openssl::sign::{Signer, Verifier};
use openssl::symm::{self, Cipher};

use {Error, Result};

pub enum KeyType {
    Public,
    Private,
}

/// An ecc key either saved by openssl or by tomcrypt.
///
/// The curve of this key is P-256, or PRIME256v1 as it is called by openssl.
pub enum EccKey {
    OpensslPublic(EcKey<Public>),
    OpensslPrivate(EcKey<Private>),
}

impl fmt::Debug for EccKey {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            EccKey::OpensslPublic(_) => write!(f, "OpensslPublic(")?,
            EccKey::OpensslPrivate(_) => write!(f, "OpensslPrivate(")?,
        }
        match self.key_type() {
            KeyType::Private => write!(f, "{})", self.to_ts_private()
                .unwrap_or_default())?,
            KeyType::Public => write!(f, "{})", self.to_ts_public()
                .unwrap_or_default())?,
        }
        Ok(())
    }
}

impl EccKey {
    /// Create a new key key pair.
    pub fn create() -> Result<Self> {
        let group = EcGroup::from_curve_name(Nid::X9_62_PRIME256V1)?;
        Ok(EccKey::OpensslPrivate(EcKey::generate(&group)?))
    }

    /// From base64 encoded tomcrypt key.
    pub fn from_ts(data: &str) -> Result<Self> {
        Self::from_tomcrypt(&base64::decode(data)?)
    }

    pub fn from_tomcrypt(data: &[u8]) -> Result<Self> {
        // Read tomcrypt DER
        Ok(::yasna::parse_der(data, |reader| {
            reader.read_sequence(|reader| {
                let f = reader.next().read_bitvec()?;
                if f.len() != 1 {
                    return Err(::yasna::ASN1Error::new(
                            ::yasna::ASN1ErrorKind::Invalid));
                }

                let key_size = reader.next().read_u16()? as usize;
                let pubkey_x = reader.next().read_biguint()?;
                let pubkey_y = reader.next().read_biguint()?;

                let secret = if f[0] {
                    Some(reader.next().read_biguint()?)
                } else {
                    None
                };

                let res: Result<Self> = (|| if let Some(secret) = secret {
                    // Private key
                    let der = ::yasna::construct_der(|writer| {
                        writer.write_sequence(|writer| {
                            // version
                            writer.next().write_u8(1);
                            // privateKey
                            writer.next().write_bytes(&secret.to_bytes_be());
                            // parameters
                            let tag = ::yasna::Tag::context(0);
                            writer.next().write_tagged(tag, |writer| {
                                writer.write_oid(
                                    #[cfg_attr(feature = "cargo-clippy",
                                       allow(unreadable_literal))]
                                    &::yasna::models::ObjectIdentifier::
                                    from_slice(&[1, 2, 840, 10045, 3, 1, 7]));
                            });
                            // Skipt the publicKey as it is not needed
                        })
                    });
                    let k = EcKey::private_key_from_der(&der)?;
                    Ok(EccKey::OpensslPrivate(k))
                } else {
                    // Public key

                    // TODO use from_public_key_affine_coordinates
                    // Convert public key to octet format is specified in RFC 5480 which
                    // delegates to [SEC1] (available here atm: http://www.secg.org/sec1-v2.pdf).
                    // Use the non-compressed form
                    let mut octets = Vec::new();
                    octets.push(4);
                    let x = pubkey_x.to_bytes_be();
                    if x.len() < key_size {
                        let len = octets.len() + key_size - x.len();
                        octets.resize(len, 0);
                    }
                    octets.extend_from_slice(&x);
                    let y = pubkey_y.to_bytes_be();
                    if y.len() < key_size {
                        let len = octets.len() + key_size - y.len();
                        octets.resize(len, 0);
                    }
                    octets.extend_from_slice(&y);

                    let group = EcGroup::from_curve_name(Nid::X9_62_PRIME256V1)?;
                    let mut ctx = BigNumContext::new()?;
                    let point = EcPoint::from_bytes(&group, &octets, &mut ctx)?;
                    let k = EcKey::from_public_key(&group, &point)?;
                    Ok(EccKey::OpensslPublic(k))
                })();
                Ok(res)
            })
        })??)
    }

    /// Convert to base64 encoded public tomcrypt key.
    pub fn to_ts_public(&self) -> Result<String> {
        Ok(base64::encode(&self.to_tomcrypt_public()?))
    }

    pub fn to_tomcrypt_public(&self) -> Result<Vec<u8>> {
        let pubkey = match *self {
            EccKey::OpensslPrivate(ref key) => key.public_key(),
            EccKey::OpensslPublic(ref key) => key.public_key(),
        };
        let group = EcGroup::from_curve_name(Nid::X9_62_PRIME256V1)?;
        let mut ctx = BigNumContext::new()?;
        let pubkey_bin = pubkey.to_bytes(&group,
            ec::PointConversionForm::UNCOMPRESSED, &mut ctx)?;
        let pub_len = (pubkey_bin.len() - 1) / 2;
        let pubkey_x = BigUint::from_bytes_be(&pubkey_bin[1..1 + pub_len]);
        let pubkey_y = BigUint::from_bytes_be(&pubkey_bin[1 + pub_len..]);

        // Write tomcrypt DER
        let der = ::yasna::construct_der(|writer| {
            writer.write_sequence(|writer| {
                writer.next().write_bitvec(&::std::iter::once(false)
                    .collect());
                writer.next().write_u16(32);
                writer.next().write_biguint(&pubkey_x);
                writer.next().write_biguint(&pubkey_y);
            })
        });

        Ok(der)
    }

    /// Convert to base64 encoded private tomcrypt key.
    pub fn to_ts_private(&self) -> Result<String> {
        Ok(base64::encode(&self.to_tomcrypt_private()?))
    }

    pub fn to_tomcrypt_private(&self) -> Result<Vec<u8>> {
        match *self {
            EccKey::OpensslPrivate(ref key) => {
                let pubkey = key.public_key();
                let group = EcGroup::from_curve_name(Nid::X9_62_PRIME256V1)?;
                let mut ctx = BigNumContext::new()?;
                let pubkey_bin = pubkey.to_bytes(&group,
                    ec::PointConversionForm::UNCOMPRESSED, &mut ctx)?;
                let pub_len = (pubkey_bin.len() - 1) / 2;
                let pubkey_x = BigUint::from_bytes_be(&pubkey_bin[1..1 + pub_len]);
                let pubkey_y = BigUint::from_bytes_be(&pubkey_bin[1 + pub_len..]);

                let privkey = BigUint::from_bytes_be(&key.private_key()
                    .to_vec());

                // Write tomcrypt DER
                let der = ::yasna::construct_der(|writer| {
                    writer.write_sequence(|writer| {
                        writer.next().write_bitvec(&::std::iter::once(true)
                            .collect());
                        writer.next().write_u16(32);
                        writer.next().write_biguint(&pubkey_x);
                        writer.next().write_biguint(&pubkey_y);

                        writer.next().write_biguint(&privkey);
                    })
                });

                Ok(der)
            }
            _ => Err(format_err!("Key contains no private key").into()),
        }
    }

    pub fn key_type(&self) -> KeyType {
        match *self {
            EccKey::OpensslPublic(_) => KeyType::Public,
            EccKey::OpensslPrivate(_) => KeyType::Private,
        }
    }

    /// This has to be the private key, the other one has to be the public key.
    pub fn create_shared_secret(&self, other_public: &EccKey)
        -> Result<Vec<u8>> {
        let key = if let EccKey::OpensslPrivate(ref key) = *self {
            key.clone()
        } else {
            return Err(format_err!(
                "No private key found to create shared secret").into());
        };

        let k1: PKey<pkey::Private>;
        let k2: PKey<pkey::Public>;
        let privkey = PKey::from_ec_key(key)?;
        let mut deriver = Deriver::new(&privkey)?;

        match *other_public {
            EccKey::OpensslPrivate(ref key) => {
                k1 = PKey::from_ec_key(key.clone())?;
                deriver.set_peer(&k1)?;
            }
            EccKey::OpensslPublic(ref key) => {
                k2 = PKey::from_ec_key(key.clone())?;
                deriver.set_peer(&k2)?;
            }
        }

        let secret = deriver.derive_to_vec()?;
        Ok(secret)
    }

    /// # Panics
    ///
    /// If this key contains no private key.
    pub fn sign(&self, data: &[u8]) -> Result<Vec<u8>> {
        if let EccKey::OpensslPrivate(ref key) = *self {
            let pkey = PKey::from_ec_key(key.clone())?;
            let mut signer = Signer::new(Some(MessageDigest::sha256()),
                &pkey)?;
            signer.update(data)?;
            return Ok(signer.sign_to_vec()?);
        }

        panic!("No private key found");
    }

    fn verify_ossl<T>(key: EcKey<T>, data: &[u8], signature: &[u8])
        -> Result<()> where T: ::openssl::pkey::HasPublic {
        let pkey = PKey::from_ec_key(key)?;
        let mut verifier = Verifier::new(MessageDigest::sha256(), &pkey)?;

        // Data
        verifier.update(data)?;
        let res = verifier.verify(signature)?;
        if res {
            Ok(())
        } else {
            Err(Error::WrongSignature)
        }
    }

    pub fn verify(&self, data: &[u8], signature: &[u8]) -> Result<()> {
        match *self {
            EccKey::OpensslPrivate(ref key) => Self::verify_ossl(key.clone(),
                data, signature),
            EccKey::OpensslPublic(ref key) => Self::verify_ossl(key.clone(),
                data, signature),
        }
    }
}

/// This eax implementation uses AES-128 in counter mode for encryption and
/// AES-128 in CBC mode to generate the OMAC/CMAC/CBCMAC.
///
/// EAX is an AEAD (Authenticated Encryption with Associated Data) encryption
/// scheme.
pub struct Eax;

impl Eax {
    /// Encrypt and authenticate data.
    ///
    /// # Arguments
    ///
    /// - `header`: Associated data, which will also be authenticated.
    ///
    /// # Return value
    ///
    /// - tag/mac
    /// - Encrypted data
    pub fn encrypt(key: &[u8; 16], nonce: &[u8; 16], header: &[u8], data: &[u8])
        -> Result<(Vec<u8>, Vec<u8>)> {
        // https://crypto.stackexchange.com/questions/26948/eax-cipher-mode-with-nonce-equal-header
        // has an explanation of eax.

        // l = block cipher size = 128 (for AES-128) = 16 byte
        // 1. n ← OMAC(0 || Nonce)
        // (the 0 means the number zero in l bits)
        let n = Self::cmac_with_iv(key, 0, nonce)?;

        // 2. h ← OMAC(1 || Nonce)
        let h = Self::cmac_with_iv(key, 1, header)?;

        // 3. enc ← CTR(M) using n as iv
        let enc = symm::encrypt(Cipher::aes_128_ctr(), key, Some(&n), data)?;

        // 4. c ← OMAC(2 || enc)
        let c = Self::cmac_with_iv(key, 2, &enc)?;

        // 5. tag ← n ^ h ^ c
        // (^ means xor)
        let mac: Vec<_> = n.iter().zip(h.iter()).zip(c.iter()).map(
            |((n, h), c)| n ^ h ^ c).collect();

        Ok((mac, enc))
    }

    pub fn decrypt(key: &[u8; 16], nonce: &[u8; 16], header: &[u8], data: &[u8],
        mac: &[u8]) -> Result<Vec<u8>> {
        let n = Self::cmac_with_iv(key, 0, nonce)?;

        // 2. h ← OMAC(1 || Nonce)
        let h = Self::cmac_with_iv(key, 1, header)?;

        // 4. c ← OMAC(2 || enc)
        let c = Self::cmac_with_iv(key, 2, data)?;

        let mac2: Vec<_> = n.iter().zip(h.iter()).zip(c.iter()).map(
            |((n, h), c)| n ^ h ^ c).take(mac.len()).collect();

        // Check mac using secure comparison
        if !::openssl::memcmp::eq(mac, &mac2) {
            // TODO A custom error
            return Err(format_err!("Packet has wrong mac").into());
        }

        // Decrypt
        let decrypt = symm::decrypt(Cipher::aes_128_ctr(), key, Some(&n),
            data)?;
        Ok(decrypt)
    }

    /// CMAC/OMAC1
    ///
    /// To avoid constructing new buffers on the heap, an iv encoded into 16
    /// bytes is prepended inside this function.
    pub fn cmac_with_iv(key: &[u8; 16], iv: u8, data: &[u8]) -> Result<Vec<u8>> {
        let cipher = Cipher::aes_128_cbc();
        let key = PKey::cmac(&cipher, key)?;
        let mut signer = Signer::new(None, &key)?;

        signer.update(&[0; 15])?;
        signer.update(&[iv])?;
        signer.update(data)?;

        let sign = signer.sign_to_vec()?;
        Ok(sign)
    }
}
