use anyhow::{bail, Result};
use pnet_packet::ip::IpNextHeaderProtocols;
use pnet_packet::Packet;
use structopt::StructOpt;
use tsproto::algorithms as algs;
use tsproto_packets::packets::*;
use tsproto_packets::HexSlice;

#[derive(StructOpt, Debug)]
#[structopt(author, about)]
struct Args {
	/// The capture file
	file: String,
}

fn get_udp_payload(data: &[u8]) -> Result<Vec<u8>> {
	// Try to parse as ethernet packet
	if let Some(eth_pack) = pnet_packet::ethernet::EthernetPacket::new(data) {
		if let Some(ip_pack) =
			pnet_packet::ipv4::Ipv4Packet::new(eth_pack.payload())
		{
			if ip_pack.get_next_level_protocol() == IpNextHeaderProtocols::Udp {
				if let Some(udp_pack) =
					pnet_packet::udp::UdpPacket::new(ip_pack.payload())
				{
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

fn print_packet(mut data: Vec<u8>, dir: Direction) -> Result<()> {
	let packet = InPacket::try_new(dir, &data)?;

	if packet.header().packet_type() == PacketType::Command {
		if let Ok(dec) = algs::decrypt_fake(&packet) {
			let start = packet.header().data().len();
			(&mut data[start..]).copy_from_slice(&dec);
			let packet = InPacket::try_new(dir, &data)?;
			println!("Packet: {:?}", packet);
			return Ok(());
		}
	}
	println!("Packet: {:?}", packet);
	Ok(())
}

fn main() -> Result<()> {
	// Parse command line options
	let args = Args::from_args();

	let mut capture = pcap::Capture::from_file(args.file)?;

	while let Ok(packet) = capture.next() {
		match get_udp_payload(&*packet) {
			Ok(packet) => {
				println!("Packet: {}", HexSlice(&packet));
				// Try to parse as ts packet
				if print_packet(packet.clone(), Direction::S2C).is_err() {
					if print_packet(packet, Direction::C2S).is_err() {
						println!("Error, no ts packet");
					}
				}
			}
			Err(_) => {}
		}
	}

	Ok(())
}
