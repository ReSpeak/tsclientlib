#[macro_use]
extern crate failure;
extern crate pcap;
extern crate pnet_packet;
extern crate structopt;
#[macro_use]
extern crate structopt_derive;
extern crate tsproto;

use std::io::Cursor;

use pnet_packet::ip::IpNextHeaderProtocols;
use pnet_packet::Packet;
use structopt::StructOpt;
use structopt::clap::AppSettings;
use tsproto::packets;
use tsproto::packets::*;
use tsproto::algorithms as algs;

type Result<T> = std::result::Result<T, failure::Error>;

#[derive(StructOpt, Debug)]
#[structopt(raw(global_settings =
    "&[AppSettings::ColoredHelp, AppSettings::VersionlessSubcommands]"))]
struct Args {
    #[structopt(long = "file", short = "f", help = "The capture file")]
    file: String,
}

fn main() {
    real_main().unwrap();
}

fn get_udp_payload(data: &[u8]) -> Result<Vec<u8>> {
    // Try to parse as ethernet packet
    if let Some(eth_pack) = pnet_packet::ethernet::EthernetPacket::new(data) {
        if let Some(ip_pack) = pnet_packet::ipv4::Ipv4Packet::new(eth_pack.payload()) {
            if ip_pack.get_next_level_protocol() == IpNextHeaderProtocols::Udp {
                if let Some(udp_pack) = pnet_packet::udp::UdpPacket::new(ip_pack.payload()) {
                    Ok(udp_pack.payload().to_vec())
                } else {
                    bail!("Not a real udp packet")
                }
            } else {
                bail!("Not an udp packet")
            }
        } else {
            bail!("Not an ip packet")
        }
    } else {
        bail!("Not an ethernet packet")
    }
}

fn get_packet(packet: &mut Vec<u8>, is_client: bool) -> Result<packets::Packet> {
    let (header, pos) = {
        let mut r = Cursor::new(packet.as_slice());
        (
            Header::read(&is_client, &mut r)?,
            r.position() as usize,
        )
    };
    let mut udp_packet = packet.split_off(pos);

    // Try to fake decrypt the packet
    if (header.p_type & 0xf) > 8 {
        // Invalid packet type
        bail!("Unknown packet type");
    }
    println!("Header: {:?}", header);
    if header.get_type() == PacketType::Command {
        let udp_packet_bak = udp_packet.clone();
        if algs::decrypt_fake(&header, &mut udp_packet).is_err() {
            udp_packet.copy_from_slice(&udp_packet_bak);
        }
    }
    let p_data = Data::read(
        &header,
        &mut Cursor::new(udp_packet.as_slice()),
    )?;
    let packet = tsproto::packets::Packet::new(header, p_data);
    Ok(packet)
}

fn real_main() -> Result<()> {
    // Parse command line options
    let args = Args::from_args();

    let mut capture = pcap::Capture::from_file(args.file)?;

    while let Ok(packet) = capture.next() {
        match get_udp_payload(&*packet) {
            Ok(packet) => {
                println!("Packet: {:?}", tsproto::utils::HexSlice(&packet));
                // Try to parse as ts packet
                let packet = if let Ok(p) = get_packet(&mut packet.clone(), true) {
                    p
                } else if let Ok(p) = get_packet(&mut packet.clone(), false) {
                    p
                } else {
                    println!("Error, no ts packet");
                    continue;
                };

                // Check if it is an init packet
                println!("Packet: {:?}", packet);
            }
            Err(_error) => {
                //println!("Error: {:?}", _error);
            }
        }
    }

    Ok(())
}
