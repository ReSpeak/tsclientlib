use std::fmt;
use std::io::Cursor;

use Result;
use packets::{Header};

pub struct HexSlice<'a, T: fmt::LowerHex + 'a>(pub &'a [T]);

impl<'a> fmt::Debug for HexSlice<'a, u8> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "[")?;
        if let Some((l, m)) = self.0.split_last() {
            for b in m {
                if *b == 0 {
                    write!(f, "0, ")?;
                } else {
                    write!(f, "{:#02x}, ", b)?;
                }
            }
            if *l == 0 {
                write!(f, "0")?;
            } else {
                write!(f, "{:#02x}", l)?;
            }
        }
        write!(f, "]")
    }
}

pub fn read_hex(s: &str) -> Result<Vec<u8>> {
    // Detect hex format
    if s.chars().nth(2) == Some(':') {
        // Wireshark
        Ok(s.split(':').map(|s| u8::from_str_radix(s, 16))
           .collect::<::std::result::Result<Vec<_>, _>>()?)
    } else if s.starts_with("0x") {
        // Own dumps
        Ok(s[2..].split(", ").map(|s| {
            if s.starts_with("0x") {
                u8::from_str_radix(&s[2..], 16)
            } else {
                u8::from_str_radix(s, 16)
            }
        })
           .collect::<::std::result::Result<Vec<_>, _>>()?)
    } else {
        Err(format_err!("Unknown hex format").into())
    }
}

pub fn parse_packet(mut udp_packet: Vec<u8>, is_client: bool)
    -> Result<(Header, Vec<u8>)> {
    let (header, pos) = {
        let mut r = Cursor::new(&udp_packet);
        (
            Header::read(&!is_client, &mut r)?,
            r.position() as usize,
        )
    };
    let udp_packet = udp_packet.split_off(pos);
    Ok((header, udp_packet))
}
