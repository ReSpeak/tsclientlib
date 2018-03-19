//! This module contains cryptography related code.
use std::fmt;

use base64;
use num::BigUint;

use curve25519_dalek::constants;
use curve25519_dalek::edwards::{CompressedEdwardsY, EdwardsPoint};
use curve25519_dalek::scalar::Scalar;
use openssl::bn::BigNumContext;
use openssl::derive::Deriver;
use openssl::ec::{self, EcGroup, EcKey, EcPoint};
use openssl::hash::MessageDigest;
use openssl::nid::Nid;
use openssl::pkey::{PKey, Private, Public};
use openssl::sign::{Signer, Verifier};
use openssl::symm::{self, Cipher};

use {Error, Result};

pub enum KeyType {
    Public,
    Private,
}

/// A public ecc key.
///
/// The curve of this key is P-256, or PRIME256v1 as it is called by openssl.
#[derive(Clone)]
pub struct EccKeyPubP256(pub EcKey<Public>);
/// A private ecc key.
///
/// The curve of this key is P-256, or PRIME256v1 as it is called by openssl.
#[derive(Clone)]
pub struct EccKeyPrivP256(pub EcKey<Private>);

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
        // TODO Use more minimal version
        write!(f, "EccKeyPrivP256({})", self.to_ts().unwrap())
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

                if f[0] {
                    // Got a private key but expected a public key
                    return Err(::yasna::ASN1Error::new(
                        ::yasna::ASN1ErrorKind::Invalid));
                }

                let res: Result<Self> = (|| {
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
                    Ok(EccKeyPubP256(k))
                })();
                Ok(res)
            })
        })??)
    }

    /// Convert to base64 encoded public tomcrypt key.
    pub fn to_ts(&self) -> Result<String> {
        Ok(base64::encode(&self.to_tomcrypt()?))
    }

    pub fn to_tomcrypt(&self) -> Result<Vec<u8>> {
        let group = EcGroup::from_curve_name(Nid::X9_62_PRIME256V1)?;
        let mut ctx = BigNumContext::new()?;
        let pubkey_bin = self.0.public_key().to_bytes(&group,
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

    pub fn verify(self, data: &[u8], signature: &[u8]) -> Result<()> {
        let pkey = PKey::from_ec_key(self.0)?;
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
}

impl EccKeyPrivP256 {
    /// Create a new key key pair.
    pub fn create() -> Result<Self> {
        let group = EcGroup::from_curve_name(Nid::X9_62_PRIME256V1)?;
        Ok(EccKeyPrivP256(EcKey::generate(&group)?))
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

                let _key_size = reader.next().read_u16()? as usize;
                let _pubkey_x = reader.next().read_biguint()?;
                let _pubkey_y = reader.next().read_biguint()?;

                if !f[0] {
                    // Expected a private key but got a public key
                    return Err(::yasna::ASN1Error::new(
                        ::yasna::ASN1ErrorKind::Invalid));
                };
                let secret = reader.next().read_biguint()?;

                let res: Result<Self> = (|| {
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
                    Ok(EccKeyPrivP256(k))
                })();
                Ok(res)
            })
        })??)
    }

    /// Convert to base64 encoded private tomcrypt key.
    pub fn to_ts(&self) -> Result<String> {
        Ok(base64::encode(&self.to_tomcrypt()?))
    }

    pub fn to_tomcrypt(&self) -> Result<Vec<u8>> {
        let pubkey = self.0.public_key();
        let group = EcGroup::from_curve_name(Nid::X9_62_PRIME256V1)?;
        let mut ctx = BigNumContext::new()?;
        let pubkey_bin = pubkey.to_bytes(&group,
            ec::PointConversionForm::UNCOMPRESSED, &mut ctx)?;
        let pub_len = (pubkey_bin.len() - 1) / 2;
        let pubkey_x = BigUint::from_bytes_be(&pubkey_bin[1..1 + pub_len]);
        let pubkey_y = BigUint::from_bytes_be(&pubkey_bin[1 + pub_len..]);

        let privkey = BigUint::from_bytes_be(&self.0.private_key()
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

    /// This has to be the private key, the other one has to be the public key.
    pub fn create_shared_secret(self, other: EccKeyPubP256)
        -> Result<Vec<u8>> {
        let privkey = PKey::from_ec_key(self.0)?;
        let pubkey = PKey::from_ec_key(other.0)?;
        let mut deriver = Deriver::new(&privkey)?;

        deriver.set_peer(&pubkey)?;

        let secret = deriver.derive_to_vec()?;
        Ok(secret)
    }

    pub fn sign(self, data: &[u8]) -> Result<Vec<u8>> {
        let pkey = PKey::from_ec_key(self.0)?;
        let mut signer = Signer::new(MessageDigest::sha256(), &pkey)?;
        signer.update(data)?;
        Ok(signer.sign_to_vec()?)
    }

    pub fn to_pub(&self) -> EccKeyPubP256 {
        self.into()
    }
}

impl<'a> Into<EccKeyPubP256> for &'a EccKeyPrivP256 {
    fn into(self) -> EccKeyPubP256 {
        EccKeyPubP256(EcKey::from_public_key(&self.0.group(),
            &self.0.public_key()).unwrap())
    }
}

impl EccKeyPubEd25519 {
    pub fn from_bytes(data: [u8; 32]) -> Self {
        EccKeyPubEd25519(CompressedEdwardsY(data))
    }

    pub fn from_base64(data: &str) -> Result<Self> {
        let mut bs = [0; 32];
        let decoded = base64::decode(data)?;
        bs.copy_from_slice(&decoded);
        Ok(Self::from_bytes(bs))
    }

    pub fn to_base64(&self) -> String {
        let EccKeyPubEd25519(CompressedEdwardsY(ref data)) = *self;
        base64::encode(data)
    }
}

impl EccKeyPrivEd25519 {
    /// This is not used to create TeamSpeak keys, as they are not canonical.
    pub fn create() -> Result<Self> {
        Ok(EccKeyPrivEd25519(Scalar::random(&mut ::rand::OsRng::new()?)))
    }

    pub fn from_bytes(data: [u8; 32]) -> Result<Self> {
        Ok(EccKeyPrivEd25519(Scalar::from_bytes_mod_order(data)))
    }

    pub fn to_base64(&self) -> String {
        base64::encode(self.0.as_bytes())
    }

    /// This has to be the private key, the other one has to be the public key.
    pub fn create_shared_secret(&self, pub_key: &EdwardsPoint)
        -> Result<[u8; 32]> {
        let res = pub_key * self.0;
        Ok(res.compress().0)
    }
}

impl<'a> Into<EccKeyPubEd25519> for &'a EccKeyPrivEd25519 {
    fn into(self) -> EccKeyPubEd25519 {
        EccKeyPubEd25519((&constants::ED25519_BASEPOINT_TABLE * &self.0)
            .compress())
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
        let mut signer = Signer::new_without_digest(&key)?;

        signer.update(&[0; 15])?;
        signer.update(&[iv])?;
        signer.update(data)?;

        let sign = signer.sign_to_vec()?;
        Ok(sign)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_p256_priv_key() {
        EccKeyPrivP256::from_ts("MG0DAgeAAgEgAiAIXJBlj1hQbaH0Eq0DuLlCmH8bl+veTA\
            O2+k9EQjEYSgIgNnImcmKo7ls5mExb6skfK2Tw+u54aeDr0OP1ITsC/50CIA8M5nm\
            DBnmDM/gZ//4AAAAAAAAAAAAAAAAAAAAZRzOI").unwrap();
    }

    #[test]
    fn parse_ed25519_pub_key() {
        EccKeyPubEd25519::from_base64("zQ3irtRjRVCafjz9j2iz3HVVsp3M7HPNGHUPmTgS\
            QIo=").unwrap();
    }
}
