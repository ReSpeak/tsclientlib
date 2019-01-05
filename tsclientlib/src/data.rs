#![allow(dead_code)] // TODO

use std::collections::HashMap;
use std::mem;
use std::net::SocketAddr;
use std::ops::Deref;
use std::u16;

use chrono::{DateTime, Duration, Utc};
use futures::Future;
use tsproto_commands::messages::s2c::{self, InMessage, InMessages};
use tsproto_commands::*;

use crate::{InnerConnection, Error, Result};

include!(concat!(env!("OUT_DIR"), "/b2mdecls.rs"));
include!(concat!(env!("OUT_DIR"), "/facades.rs"));
include!(concat!(env!("OUT_DIR"), "/m2bdecls.rs"));
include!(concat!(env!("OUT_DIR"), "/structs.rs"));

macro_rules! max_clients {
	($cmd:ident) => {{
		let ch = if $cmd.is_max_clients_unlimited {
			None
		} else if $cmd.max_clients >= 0 && $cmd.max_clients <= u16::MAX as i32 {
			Some($cmd.max_clients as u16)
		} else {
			// Max clients is less than zero or too high so ignore it
			None
			};
		let ch_fam = if $cmd.is_max_family_clients_unlimited {
			MaxFamilyClients::Unlimited
		} else if $cmd.inherits_max_family_clients {
			MaxFamilyClients::Inherited
		} else if $cmd.max_family_clients >= 0
			&& $cmd.max_family_clients <= u16::MAX as i32
			{
			MaxFamilyClients::Limited($cmd.max_family_clients as u16)
		} else {
			// Max clients is less than zero or too high so ignore it
			MaxFamilyClients::Unlimited
			};
		(ch, ch_fam)
		}};
}

impl Connection {
	pub(crate) fn new(server_uid: Uid, msg: &InMessage) -> Self {
		let packet = if let InMessages::InitServer(p) = msg.msg() {
			p
		} else {
			panic!("Got no initserver packet in Connection::new");
		};
		let packet = packet.iter().next().unwrap();
		Self {
			own_client: packet.client_id,
			server: copy_attrs!(packet, Server;
				welcome_message,
				max_clients,
				codec_encryption_mode,
				hostmessage,
				hostmessage_mode,
				default_server_group,
				default_channel_group,
				hostbanner_url,
				hostbanner_gfx_url,
				hostbanner_gfx_interval,
				priority_speaker_dimm_modificator,
				virtual_server_id,
				hostbutton_tooltip,
				hostbutton_url,
				hostbutton_gfx_url,
				phonetic_name,
				hostbanner_mode,
				protocol_version,
				icon_id,
				temp_channel_default_delete_delay,
				;

				uid: server_uid,
				name: packet.name.into(),
				platform: packet.server_platform.into(),
				version: packet.server_version.into(),
				created: packet.server_created,
				ip: packet.server_ip.iter().map(|s| s.to_string()).collect(),
				ask_for_privilegekey: packet.ask_for_privilegekey,
				// TODO Or get from license struct for newer servers
				license: packet.license_type.unwrap_or(LicenseType::NoLicense),

				optional_data: None,
				connection_data: None,
				clients: HashMap::new(),
				channels: HashMap::new(),
				groups: HashMap::new(),
			),
		}
	}

	pub(crate) fn handle_message(&mut self, msg: &InMessage) -> Result<()> {
		self.handle_message_generated(msg)?;

		// Also raise events
		match msg {
			_ => {} // TODO
		}
		Ok(())
	}

	fn get_mut_server(&mut self) -> &mut Server { &mut self.server }
	fn add_server_group(
		&mut self,
		group: ServerGroupId,
		r: ServerGroup,
	) -> Option<ServerGroup>
	{
		self.server.groups.insert(group, r)
	}

	fn get_mut_client(&mut self, client: ClientId) -> Result<&mut Client> {
		self.server
			.clients
			.get_mut(&client)
			.ok_or_else(|| format_err!("Client {} not found", client).into())
	}
	fn add_client(&mut self, client: ClientId, r: Client) -> Option<Client> {
		self.server.clients.insert(client, r)
	}
	fn remove_client(&mut self, client: ClientId) -> Option<Client> {
		self.server.clients.remove(&client)
	}
	fn add_connection_client_data(
		&mut self,
		client: ClientId,
		r: ConnectionClientData,
	) -> Result<Option<ConnectionClientData>>
	{
		if let Some(client) = self.server.clients.get_mut(&client) {
			Ok(mem::replace(&mut client.connection_data, Some(r)))
		} else {
			Err(format_err!("Client {} not found", client).into())
		}
	}

	fn get_mut_channel(&mut self, channel: ChannelId) -> Result<&mut Channel> {
		self.server
			.channels
			.get_mut(&channel)
			.ok_or_else(|| format_err!("Channel {} not found", channel).into())
	}
	fn add_channel(
		&mut self,
		channel: ChannelId,
		r: Channel,
	) -> Option<Channel>
	{
		self.server.channels.insert(channel, r)
	}
	fn remove_channel(&mut self, channel: ChannelId) -> Option<Channel> {
		self.server.channels.remove(&channel)
	}

	// Backing functions for MessageToBook declarations

	fn return_false<T>(&self, _: T) -> bool { false }
	fn return_none<T, O>(&self, _: T) -> Option<O> { None }
	fn void_fun<T, U>(&self, _: T, _: U) {}

	fn max_clients_cc_fun(
		&self,
		cmd: &s2c::ChannelCreatedPart,
	) -> (Option<u16>, MaxFamilyClients)
	{
		max_clients!(cmd)
	}
	fn max_clients_ce_fun(
		&mut self,
		channel_id: ChannelId,
		cmd: &s2c::ChannelEditedPart,
	)
	{
		if let Ok(channel) = self.get_mut_channel(channel_id) {
			let (ch, ch_fam) = max_clients!(cmd);
			channel.max_clients = ch;
			channel.max_family_clients = ch_fam;
		}
	}
	fn max_clients_cl_fun(
		&self,
		cmd: &s2c::ChannelListPart,
	) -> (Option<u16>, MaxFamilyClients)
	{
		let ch = if cmd.is_max_clients_unlimited {
			None
		} else if cmd.max_clients >= 0 && cmd.max_clients <= u16::MAX as i32 {
			Some(cmd.max_clients as u16)
		} else {
			// Max clients is less than zero or too high so ignore it
			None
		};
		let ch_fam = if cmd.is_max_family_clients_unlimited {
			MaxFamilyClients::Unlimited
		} else if cmd.inherits_max_family_clients {
			MaxFamilyClients::Inherited
		} else if cmd.max_family_clients >= 0
			&& cmd.max_family_clients <= u16::MAX as i32
		{
			MaxFamilyClients::Limited(cmd.max_family_clients as u16)
		} else {
			// Max clients is less than zero or too high so ignore it
			MaxFamilyClients::Unlimited
		};
		(ch, ch_fam)
	}

	fn channel_type_cc_fun(
		&self,
		cmd: &s2c::ChannelCreatedPart,
	) -> ChannelType
	{
		if cmd.is_permanent == Some(true) {
			ChannelType::Permanent
		} else if cmd.is_semi_permanent == Some(true) {
			ChannelType::SemiPermanent
		} else {
			ChannelType::Temporary
		}
	}

	fn channel_type_ce_fun(
		&mut self,
		channel_id: ChannelId,
		cmd: &s2c::ChannelEditedPart,
	)
	{
		if let Ok(channel) = self.get_mut_channel(channel_id) {
			let typ = if cmd.is_permanent == Some(true) {
				ChannelType::Permanent
			} else if cmd.is_semi_permanent == Some(true) {
				ChannelType::SemiPermanent
			} else {
				ChannelType::Temporary
			};
			channel.channel_type = typ;
		}
	}

	fn channel_type_cl_fun(&self, cmd: &s2c::ChannelListPart) -> ChannelType {
		if cmd.is_permanent {
			ChannelType::Permanent
		} else if cmd.is_semi_permanent {
			ChannelType::SemiPermanent
		} else {
			ChannelType::Temporary
		}
	}

	fn away_fun(&self, cmd: &s2c::ClientEnterViewPart) -> Option<String> {
		if cmd.is_away {
			Some(cmd.away_message.into())
		} else {
			None
		}
	}

	fn talk_power_fun(
		&self,
		cmd: &s2c::ClientEnterViewPart,
	) -> Option<TalkPowerRequest>
	{
		if cmd.talk_power_request_time.timestamp() > 0 {
			Some(TalkPowerRequest {
				time: cmd.talk_power_request_time,
				message: cmd.talk_power_request_message.into(),
			})
		} else {
			None
		}
	}

	fn badges_fun(&self, cmd: &s2c::ClientEnterViewPart) -> Vec<String> {
		cmd.badges.split(',').map(|s| s.into()).collect()
	}

	fn address_fun(
		&self,
		cmd: &s2c::ClientConnectionInfoPart,
	) -> Option<SocketAddr>
	{
		let ip = if let Ok(ip) = cmd.ip.parse() {
			ip
		} else {
			return None;
		};
		Some(SocketAddr::new(ip, cmd.port))
	}
}
