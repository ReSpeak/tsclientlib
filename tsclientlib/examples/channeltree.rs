use anyhow::{bail, Result};
use futures::prelude::*;
use structopt::StructOpt;
use tokio::time::{self, Duration};

use tsclientlib::data::{self, Channel, Client};
use tsclientlib::{ChannelId, ConnectOptions, Connection, DisconnectOptions, Identity, StreamItem};

#[derive(StructOpt, Debug)]
#[structopt(author, about)]
struct Args {
	/// The address of the server to connect to
	#[structopt(short = "a", long, default_value = "localhost")]
	address: String,
	/// Print the content of all packets
	///
	/// 0. Print nothing
	/// 1. Print command string
	/// 2. Print packets
	/// 3. Print udp packets
	#[structopt(short = "v", long, parse(from_occurrences))]
	verbose: u8,
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

fn print_channel_tree(con: &data::Connection) {
	let mut channels: Vec<_> = con.channels.values().collect();
	let mut clients: Vec<_> = con.clients.values().collect();
	// This is not the real sorting order, the order is the ChannelId of the
	// channel on top of this one, but we don't care for this example.
	channels.sort_by_key(|ch| ch.order.0);
	clients.sort_by_key(|c| c.talk_power);
	println!("{}", con.server.name);
	print_channels(&clients, &channels, ChannelId(0), 0);
}

#[tokio::main]
async fn main() -> Result<()> { real_main().await }

async fn real_main() -> Result<()> {
	// Parse command line options
	let args = Args::from_args();

	let con_config = ConnectOptions::new(args.address)
		.log_commands(args.verbose >= 1)
		.log_packets(args.verbose >= 2)
		.log_udp_packets(args.verbose >= 3);

	// Optionally set the key of this client, otherwise a new key is generated.
	let id = Identity::new_from_str(
		"MG0DAgeAAgEgAiAIXJBlj1hQbaH0Eq0DuLlCmH8bl+veTAO2+\
		k9EQjEYSgIgNnImcmKo7ls5mExb6skfK2Tw+u54aeDr0OP1ITs\
		C/50CIA8M5nmDBnmDM/gZ//4AAAAAAAAAAAAAAAAAAAAZRzOI").unwrap();
	let con_config = con_config.identity(id);

	// Connect
	let mut con = Connection::new(con_config)?;

	let r = con
		.events()
		.try_filter(|e| future::ready(matches!(e, StreamItem::ConEvents(_))))
		.next()
		.await;
	if let Some(r) = r {
		r?;
	}

	con.get_mut_state().unwrap().get_server().set_subscribed(true)?;

	// Wait some time
	let mut events = con.events().try_filter(|_| future::ready(false));
	tokio::select! {
		_ = &mut time::delay_for(Duration::from_secs(1)) => {}
		_ = events.next() => {
			bail!("Disconnected");
		}
	};
	drop(events);

	// Print channel tree
	print_channel_tree(con.get_state().unwrap());

	// Change name
	{
		let mut state = con.get_mut_state().unwrap();
		let name = state.clients[&state.own_client].name.clone();
		state.set_name(&format!("{}1", name))?;
		state.set_input_muted(true)?;
	}

	// Wait some time
	let mut events = con.events().try_filter(|_| future::ready(false));
	tokio::select! {
		_ = &mut time::delay_for(Duration::from_secs(3)) => {}
		_ = events.next() => {
			bail!("Disconnected");
		}
	};
	drop(events);

	// Disconnect
	con.disconnect(DisconnectOptions::new())?;
	con.events().for_each(|_| future::ready(())).await;

	Ok(())
}
