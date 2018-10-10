#[macro_use]
extern crate failure;
extern crate futures;
extern crate structopt;
extern crate tokio;
extern crate tsclientlib;
extern crate tsproto;

use std::time::{Duration, Instant};

use futures::{Future, Sink};
use structopt::clap::AppSettings;
use structopt::StructOpt;
use tokio::timer::Delay;

use tsclientlib::{ChannelId, ConnectOptions, Connection, DisconnectOptions, Reason};
use tsclientlib::data::{Channel, Client};
use tsproto::commands::Command;
use tsproto::packets::{self, Header, PacketType};

#[derive(StructOpt, Debug)]
#[structopt(raw(
	global_settings = "&[AppSettings::ColoredHelp, \
	                   AppSettings::VersionlessSubcommands]"
))]
struct Args {
	#[structopt(
		short = "a",
		long = "address",
		default_value = "localhost",
		help = "The address of the server to connect to"
	)]
	address: String,
	#[structopt(
		short = "v",
		long = "verbose",
		help = "Print the content of all packets",
		parse(from_occurrences)
	)]
	verbose: u8,
	// 0. Print nothing
	// 1. Print command string
	// 2. Print packets
	// 3. Print udp packets
}

/// `channels` have to be ordered.
fn print_channels(clients: &[&Client], channels: &[&Channel], parent: ChannelId, depth: usize) {
	let indention = "  ".repeat(depth);
	for channel in channels {
		if channel.parent == parent {
			println!("{}- {}", indention, channel.name);
			// Print all clients in this channel
			for client in clients {
				if client.channel == channel.id {
					println!("{}  {}", indention, client.name);
				}
			}

			print_channels(clients, channels, channel.id, depth + 1);
		}
	}
}

fn main() -> Result<(), failure::Error> {
	// Parse command line options
	let args = Args::from_args();

	tokio::run(
		futures::lazy(|| {
			let con_config = ConnectOptions::new(args.address)
				// TODO log commands in tsproto
				.log_commands(args.verbose >= 1)
				.log_packets(args.verbose >= 2)
				.log_udp_packets(args.verbose >= 3);

			// Optionally set the key of this client, otherwise a new key is generated.
			let con_config = con_config.private_key_ts(
				"MG0DAgeAAgEgAiAIXJBlj1hQbaH0Eq0DuLlCmH8bl+veTAO2+\
				k9EQjEYSgIgNnImcmKo7ls5mExb6skfK2Tw+u54aeDr0OP1ITs\
				C/50CIA8M5nmDBnmDM/gZ//4AAAAAAAAAAAAAAAAAAAAZRzOI").unwrap();

			// Connect
			Connection::new(con_config)
		}).and_then(|con| {
			{
				let con = con.lock();
				println!(
					"Server welcome message: {}",
					sanitize(&con.server.welcome_message)
				);
			}
			let cmd = Command::new("channelsubscribeall");
			let header = Header::new(PacketType::Command);
			let data = packets::Data::Command(cmd);
			let packet = packets::Packet::new(header, data);

			// Send a message and wait until we get an answer for the return code
			con.get_packet_sink().send(packet).map(|_| con)
		}).and_then(|con| {

			// Wait some time
			Delay::new(Instant::now() + Duration::from_secs(1))
				.map(move |_| con)
				.map_err(|e| format_err!("Failed to wait ({:?})", e).into())
		}).and_then(|con| {
			// Print channel tree
			{
				let con = con.lock();
				let mut channels: Vec<_> = con.server.channels.values().collect();
				let mut clients: Vec<_> = con.server.clients.values().collect();
				channels.sort_by_key(|ch| ch.order);
				clients.sort_by_key(|c| c.talk_power);
				println!("{}", con.server.name);
				print_channels(&clients, &channels, ChannelId(0), 0);
			}

			// Disconnect
			con.disconnect(
				DisconnectOptions::new()
					.reason(Reason::Clientdisconnect)
					.message("Is this the real world?"),
			)
		}).map_err(|e| panic!("An error occurred {:?}", e)),
	);

	Ok(())
}

/// Only retain a certain set of characters.
fn sanitize(s: &str) -> String {
	s.chars()
		.filter(|c| {
			c.is_alphanumeric() || [
				' ', '\t', '.', ':', '-', '_', '"', '\'', '/', '(', ')', '[',
				']', '{', '}',
			]
				.contains(c)
		}).collect()
}
