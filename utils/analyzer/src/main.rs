use failure::bail;
use pnet_packet::ip::IpNextHeaderProtocols;
use pnet_packet::Packet;
use structopt::StructOpt;
use tsproto::algorithms as algs;
use tsproto_packets::packets::*;
use tsproto_packets::HexSlice;

type Result<T> = std::result::Result<T, failure::Error>;

#[derive(StructOpt, Debug)]
#[structopt(author, about)]
struct Args {
	/// The capture file
	#[structopt(short = "f", long)]
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

fn get_packet(data: &[u8], dir: Direction) -> Result<InPacket> {
	let mut packet = InPacket::try_new(data.into(), dir)?;
	let header = packet.header();

	if header.packet_type() == PacketType::Command {
		if let Ok(dec) = algs::decrypt_fake(&packet) {
			packet.set_content(dec);
		}
	}
	Ok(packet)
}

fn main() -> Result<()> {
	// Parse command line options
	let args = Args::from_args();

	let mut capture = pcap::Capture::from_file(args.file)?;

	while let Ok(packet) = capture.next() {
		match get_udp_payload(&*packet) {
			Ok(packet) => {
				println!("Packet: {:?}", HexSlice(&packet));
				// Try to parse as ts packet
				let packet = if let Ok(p) = get_packet(&packet, Direction::S2C)
				{
					p
				} else if let Ok(p) = get_packet(&packet, Direction::C2S) {
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
