//! Handle packet splitting and cryptography
use std::u64;

use {tomcrypt, base64};
use byteorder::{NetworkEndian, WriteBytesExt};
use num::BigUint;
use quicklz::CompressionLevel;
use ring::digest;

use Result;
use packets::*;

pub fn must_encrypt(t: PacketType) -> bool {
    match t {
        PacketType::Command | PacketType::CommandLow => true,
        PacketType::Voice |
        PacketType::Ack |
        PacketType::AckLow |
        PacketType::VoiceWhisper |
        PacketType::Ping |
        PacketType::Pong |
        PacketType::Init => false,
    }
}

pub fn should_encrypt(t: PacketType, voice_encryption: bool) -> bool {
    must_encrypt(t) || t == PacketType::Ack || t == PacketType::AckLow
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
pub fn compress_and_split(packet: &Packet) -> Vec<(Header, Vec<u8>)> {
    // Everything else (except whisper packets) have to be less than 500 bytes
    let header_size = {
        let mut buf = Vec::new();
        packet.header.write(&mut buf).unwrap();
        buf.len()
    };
    let mut data = Vec::new();
    packet.data.write(&mut data).unwrap();
    // The maximum packet size (including header) is 500 bytes.
    let max_size = 500 - header_size;
    // Split the data if it is necessary.
    // Compress also slightly smaller packets
    let (datas, compressed) = if data.len() > (max_size - 100) {
        // Compress with QuickLZ
        let cdata = ::quicklz::compress(&data, CompressionLevel::Lvl1);
        // Use only if it is efficient
        let (mut data, compressed) = if cdata.len() > data.len() {
            (data, false)
        } else {
            (cdata, true)
        };

        // Ignore size limit for whisper packets
        if data.len() <= max_size
            || packet.header.get_type() == PacketType::VoiceWhisper
        {
            (vec![data], compressed)
        } else {
            // Split
            let count = (data.len() + max_size - 1) / max_size;
            let mut splitted = Vec::with_capacity(count);
            // Split from the back so the buffer does not have to be moved each
            // time.
            // Rest
            let mut len = data.len();
            splitted.push(data.split_off(len - (len % max_size)));

            while {
                len = data.len();
                len > 0
            } {
                splitted.push(data.split_off(len - max_size));
            }
            (splitted, compressed)
        }
    } else {
        (vec![data], false)
    };
    let len = datas.len();
    let fragmented = len > 1;
    let default_header = {
        let mut h = Header::default();
        h.set_type(packet.header.get_type());
        h
    };
    let mut packets = Vec::with_capacity(datas.len());
    for (i, d) in datas.into_iter().rev().enumerate() {
        let mut h = default_header.clone();
        // Only set flags on first fragment
        if i == 0 && compressed {
            h.set_compressed(true);
        }

        // Set fragmented flag on first and last part
        if fragmented && (i == 0 || i == len - 1) {
            h.set_fragmented(true);
        }
        packets.push((h, d));
    }
    packets
}

fn create_key_nonce(
    header: &Header,
    generation_id: u32,
    iv: &[u8; 20],
) -> ([u8; 16], [u8; 16]) {
    let mut temp = [0; 26];
    if header.c_id.is_some() {
        temp[0] = 0x31;
    } else {
        temp[0] = 0x30;
    }
    temp[1] = header.p_type & 0xf;
    let mut buf = Vec::with_capacity(4);
    buf.write_u32::<NetworkEndian>(generation_id).unwrap();
    temp[2..6].copy_from_slice(&buf);
    temp[6..].copy_from_slice(iv);

    let keynonce = digest::digest(&digest::SHA256, &temp);
    let keynonce = keynonce.as_ref();
    let mut key = [0; 16];
    let mut nonce = [0; 16];
    key.copy_from_slice(&keynonce[..16]);
    nonce.copy_from_slice(&keynonce[16..]);
    key[0] ^= (header.p_id >> 8) as u8;
    key[1] ^= (header.p_id & 0xff) as u8;
    (key, nonce)
}

pub fn encrypt_key_nonce(
    header: &mut Header,
    data: &mut [u8],
    key: &[u8; 16],
    nonce: &[u8; 16],
) -> Result<()> {
    let mut meta = Vec::with_capacity(5);
    header.write_meta(&mut meta)?;

    let mut eax =
        tomcrypt::EaxState::new(tomcrypt::rijndael(), key, nonce, Some(&meta))?;

    eax.encrypt_in_place(data)?;
    header.mac.copy_from_slice(&eax.finish(8)?);

    Ok(())
}

pub fn encrypt_fake(header: &mut Header, data: &mut [u8]) -> Result<()> {
    encrypt_key_nonce(header, data, &::FAKE_KEY, &::FAKE_NONCE)
}

pub fn encrypt(
    header: &mut Header,
    data: &mut [u8],
    generation_id: u32,
    iv: &[u8; 20],
) -> Result<()> {
    // TODO Cache this more efficiently for a generation
    let (key, nonce) = create_key_nonce(header, generation_id, iv);
    encrypt_key_nonce(header, data, &key, &nonce)
}

pub fn decrypt_key_nonce(
    header: &Header,
    data: &mut [u8],
    key: &[u8; 16],
    nonce: &[u8; 16],
) -> Result<()> {
    let mut meta = Vec::with_capacity(5);
    header.write_meta(&mut meta)?;

    let mut eax =
        tomcrypt::EaxState::new(tomcrypt::rijndael(), key, nonce, Some(&meta))?;

    eax.decrypt_in_place(data)?;
    if !header.mac.iter().eq(&eax.finish(8)?) {
        Err("Packet has wrong mac".into())
    } else {
        Ok(())
    }
}

pub fn decrypt_fake(header: &Header, data: &mut [u8]) -> Result<()> {
    decrypt_key_nonce(header, data, &::FAKE_KEY, &::FAKE_NONCE)
}

pub fn decrypt(
    header: &Header,
    data: &mut [u8],
    generation_id: u32,
    iv: &[u8; 20],
) -> Result<()> {
    let (key, nonce) = create_key_nonce(header, generation_id, iv);
    decrypt_key_nonce(header, data, &key, &nonce)
}

/// Compute shared iv and shared mac.
pub fn compute_iv_mac(
    alpha: &[u8; 10],
    beta: &[u8; 10],
    our_key: &mut tomcrypt::EccKey,
    other_key: &mut tomcrypt::EccKey,
) -> Result<([u8; 20], [u8; 8])> {
    let shared_secret =
        tomcrypt::EccKey::create_shared_secret(our_key, other_key, 32)?;
    let mut shared_iv = [0; 20];
    shared_iv.copy_from_slice(digest::digest(&digest::SHA1, &shared_secret)
                              .as_ref());
    for i in 0..10 {
        shared_iv[i] ^= alpha[i];
    }
    for i in 0..10 {
        shared_iv[i + 10] ^= beta[i];
    }
    let mut shared_mac = [0; 8];
    shared_mac.copy_from_slice(
        &digest::digest(&digest::SHA1, &shared_iv).as_ref()[..8],
    );
    Ok((shared_iv, shared_mac))
}

pub fn hash_cash(key: &mut tomcrypt::EccKey, level: u8) -> Result<u64> {
    let omega = base64::encode(&key.export_public()?);
    let mut offset = 0;
    while offset < u64::MAX && get_hash_cash_level(&omega, offset) < level {
        offset += 1;
    }
    Ok(offset)
}

pub fn get_hash_cash_level(omega: &str, offset: u64) -> u8 {
    let data = digest::digest(
        &digest::SHA1,
        format!("{}{}", omega, offset).as_bytes(),
    );
    let mut res = 0;
    for &d in data.as_ref() {
        if d == 0 {
            res += 8;
        } else {
            res += d.leading_zeros() as u8;
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

pub fn array_to_biguint(i: &[u8; 64]) -> BigUint {
    BigUint::from_bytes_be(i)
}

#[cfg(test)]
mod tests {
    use algorithms::*;
    use packets::{Data, Header, PacketType};

    #[test]
    fn test_fake_crypt() {
        ::init().unwrap();
        let data = (0..100).into_iter().collect::<Vec<_>>();
        let mut header = Header::default();
        let mut enc_data = data.clone();
        encrypt_fake(&mut header, &mut enc_data).unwrap();
        let mut dec_data = enc_data.clone();
        decrypt_fake(&header, &mut dec_data).unwrap();
        assert_eq!(&data, &dec_data);
    }

    #[test]
    fn test_fake_encrypt() {
        let data = Data::Ack(0);
        let mut p_data = Vec::new();
        data.write(&mut p_data).unwrap();
        let mut header = Header::default();
        header.c_id = Some(0);
        header.set_type(PacketType::Ack);
        encrypt_fake(&mut header, &mut p_data).unwrap();

        let mut buf = Vec::new();
        header.write(&mut buf).unwrap();
        buf.append(&mut p_data);
        let real_res: &[u8] = &[
            0xa4,
            0x7b,
            0x47,
            0x94,
            0xdb,
            0xa9,
            0x6a,
            0xc5,
            0,
            0,
            0,
            0,
            0x6,
            0xfe,
            0x18,
        ];
        assert_eq!(real_res, buf.as_slice());
    }
}
