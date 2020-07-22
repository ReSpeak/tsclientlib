use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, SocketAddr};
use std::{iter, mem, u16};

use serde::{Deserialize, Serialize};
use slog::{debug, Logger};
use time::{Duration, OffsetDateTime};
use tsproto_packets::packets::OutCommand;
use tsproto_types::crypto::EccKeyPubP256;
use tsproto_types::*;

use crate::events::{Event, ExtraInfo, PropertyId, PropertyValue, PropertyValueRef};
use crate::messages::s2c::InMessage;
use crate::messages::{c2s, s2c};
use crate::{Error, MessageTarget, Result};

include!(concat!(env!("OUT_DIR"), "/m2bdecls.rs"));
include!(concat!(env!("OUT_DIR"), "/structs.rs"));
include!(concat!(env!("OUT_DIR"), "/properties.rs"));

pub mod exts {
	use super::*;

	include!(concat!(env!("OUT_DIR"), "/b2mdecls.rs"));
}

macro_rules! max_clients {
	($msg:ident) => {{
		if $msg.is_max_clients_unlimited.unwrap_or_default() {
			Some(MaxClients::Unlimited)
		} else if $msg.max_clients.map(|i| i >= 0 && i <= u16::MAX as i32).unwrap_or_default() {
			Some(MaxClients::Limited($msg.max_clients.unwrap() as u16))
		} else {
			// Max clients is less than zero or too high so ignore it
			None
			}
		}};
}

macro_rules! copy_attrs {
	($from:ident, $to:ident; $($attr:ident),* $(,)*; $($extra:ident: $ex:expr),* $(,)*) => {
		$to {
			$($attr: $from.$attr.clone(),)*
			$($extra: $ex,)*
		}
	};
}

impl Connection {
	pub fn new(public_key: EccKeyPubP256, msg: &s2c::InInitServer) -> Self {
		let packet = msg.iter().next().unwrap();
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
				hostbanner_mode,
				protocol_version,
				icon_id,
				temp_channel_default_delete_delay,
				;

				public_key: public_key,
				name: packet.server_name.clone(),
				phonetic_name: packet.server_phonetic_name.clone(),
				nickname: packet.nickname.clone(),
				platform: packet.server_platform.clone(),
				version: packet.server_version.clone(),
				created: packet.server_created,
				ips: packet.server_ip.clone().unwrap_or_else(Vec::new),
				ask_for_privilegekey: packet.ask_for_privilegekey,
				// TODO Get from license struct
				license: LicenseType::NoLicense,

				optional_data: None,
				connection_data: None,
			),
			clients: HashMap::new(),
			channels: HashMap::new(),
			channel_groups: HashMap::new(),
			server_groups: HashMap::new(),
		}
	}

	pub fn handle_command(&mut self, logger: &Logger, msg: &s2c::InMessage) -> Result<Vec<Event>> {
		// Returns if it handled the message so we can warn if a message is
		// unhandled.
		let (mut handled, mut events) = self.handle_command_generated(msg)?;
		// Handle special messages
		match msg {
			InMessage::TextMessage(msg) => {
				for msg in msg.iter() {
					let target = match msg.target {
						TextMessageTargetMode::Server => MessageTarget::Server,
						TextMessageTargetMode::Channel => MessageTarget::Channel,
						TextMessageTargetMode::Client => {
							let client = if let Some(client) = msg.target_client_id {
								client
							} else {
								return Err(Error::MessageWithoutTargetClientId);
							};
							MessageTarget::Client(client)
						}
						TextMessageTargetMode::Unknown => {
							return Err(Error::UnknownTextMessageTargetMode);
						}
					};
					events.push(Event::Message {
						target,
						invoker: Invoker {
							name: msg.invoker_name.clone(),
							id: msg.invoker_id,
							uid: msg.invoker_uid.clone(),
						},
						message: msg.message.to_string(),
					});
					handled = true;
				}
			}
			InMessage::ClientPoke(msg) => {
				for msg in msg.iter() {
					events.push(Event::Message {
						target: MessageTarget::Poke(msg.invoker_id),
						invoker: Invoker {
							name: msg.invoker_name.clone(),
							id: msg.invoker_id,
							uid: msg.invoker_uid.clone(),
						},
						message: msg.message.to_string(),
					});
					handled = true;
				}
			}
			InMessage::CommandError(_) => handled = true,
			InMessage::ChannelListFinished(_) => {
				events.push(Event::ChannelListFinished);
				handled = true;
			}
			_ => {}
		}

		if let Some(invoker) = events.first().and_then(Event::get_invoker) {
			// If we know this client and the name change, adjust the name.
			if let Ok(client) = self.get_mut_client(invoker.id) {
				if client.name != invoker.name {
					let old = mem::replace(&mut client.name, invoker.name.clone());
					events.push(Event::PropertyChanged {
						id: PropertyId::ClientName(client.id),
						old: PropertyValue::String(old),
						invoker: None,
						extra: ExtraInfo { reason: None },
					});
				}
			}
		}

		if !handled {
			debug!(logger, "Unhandled message"; "message" => msg.get_command_name());
		}

		Ok(events)
	}

	fn get_server(&self) -> Result<&Server> { Ok(&self.server) }
	fn get_mut_server(&mut self) -> Result<&mut Server> { Ok(&mut self.server) }

	fn get_channel_group(&self, group: ChannelGroupId) -> Result<&ChannelGroup> {
		self.channel_groups.get(&group).ok_or_else(|| Error::NotFound("ChannelGroup", group.to_string()))
	}
	fn add_channel_group(
		&mut self, group: ChannelGroupId, r: ChannelGroup, _: &mut Vec<Event>,
	) -> Result<Option<ChannelGroup>> {
		Ok(self.channel_groups.insert(group, r))
	}
	fn get_server_group(&self, group: ServerGroupId) -> Result<&ServerGroup> {
		self.server_groups.get(&group).ok_or_else(|| Error::NotFound("ServerGroup", group.to_string()))
	}
	fn add_server_group(
		&mut self, group: ServerGroupId, r: ServerGroup, _: &mut Vec<Event>,
	) -> Result<Option<ServerGroup>> {
		Ok(self.server_groups.insert(group, r))
	}

	fn get_optional_server_data(&self) -> Result<&OptionalServerData> {
		self.server.optional_data.as_ref().ok_or(Error::None)
	}

	fn get_connection_server_data(&self) -> Result<&ConnectionServerData> {
		self.server.connection_data.as_ref().ok_or(Error::None)
	}

	fn get_connection(&self) -> Result<&Connection> { Ok(&self) }

	fn get_client(&self, client: ClientId) -> Result<&Client> {
		self.clients.get(&client).ok_or_else(|| Error::NotFound("Client", client.to_string()))
	}
	fn get_mut_client(&mut self, client: ClientId) -> Result<&mut Client> {
		self.clients.get_mut(&client).ok_or_else(|| Error::NotFound("Client", client.to_string()))
	}
	fn add_client(
		&mut self, client: ClientId, r: Client, _: &mut Vec<Event>,
	) -> Result<Option<Client>> {
		Ok(self.clients.insert(client, r))
	}
	fn remove_client(&mut self, client: ClientId, _: &mut Vec<Event>) -> Result<Option<Client>> {
		Ok(self.clients.remove(&client))
	}

	fn get_connection_client_data(&self, client: ClientId) -> Result<&ConnectionClientData> {
		if let Some(c) = self.clients.get(&client) {
			c.connection_data.as_ref().ok_or(Error::None)
		} else {
			Err(Error::NotFound("Client", client.to_string()))
		}
	}
	fn add_connection_client_data(
		&mut self, client: ClientId, r: ConnectionClientData, _: &mut Vec<Event>,
	) -> Result<Option<ConnectionClientData>> {
		if let Some(client) = self.clients.get_mut(&client) {
			Ok(mem::replace(&mut client.connection_data, Some(r)))
		} else {
			Err(Error::NotFound("Client", client.to_string()))
		}
	}

	fn get_optional_client_data(&self, client: ClientId) -> Result<&OptionalClientData> {
		if let Some(c) = self.clients.get(&client) {
			c.optional_data.as_ref().ok_or(Error::None)
		} else {
			Err(Error::NotFound("Client", client.to_string()))
		}
	}

	fn get_channel(&self, channel: ChannelId) -> Result<&Channel> {
		self.channels.get(&channel).ok_or_else(|| Error::NotFound("Channel", channel.to_string()))
	}
	fn get_mut_channel(&mut self, channel: ChannelId) -> Result<&mut Channel> {
		self.channels
			.get_mut(&channel)
			.ok_or_else(|| Error::NotFound("Channel", channel.to_string()))
	}
	fn add_channel(
		&mut self, channel: ChannelId, r: Channel, events: &mut Vec<Event>,
	) -> Result<Option<Channel>> {
		self.channel_order_insert(r.id, r.order, r.parent, events);
		Ok(self.channels.insert(channel, r))
	}
	fn remove_channel(
		&mut self, channel: ChannelId, events: &mut Vec<Event>,
	) -> Result<Option<Channel>> {
		let old = self.channels.remove(&channel);
		if let Some(ch) = &old {
			self.channel_order_remove(ch.id, ch.order, events);
		}
		Ok(old)
	}

	fn get_optional_channel_data(&self, channel: ChannelId) -> Result<&OptionalChannelData> {
		if let Some(c) = self.channels.get(&channel) {
			c.optional_data.as_ref().ok_or(Error::None)
		} else {
			Err(Error::NotFound("Channel", channel.to_string()))
		}
	}

	fn get_file(&self, _channel: ChannelId, _path: &str) -> Result<&File> {
		unimplemented!("Files are not yet implemented")
	}

	// Backing functions for MessageToBook declarations

	fn return_false<T>(&self, _: T, _: &mut Vec<Event>) -> Result<bool> { Ok(false) }
	fn return_none<T, O>(&self, _: T, _: &mut Vec<Event>) -> Result<Option<O>> { Ok(None) }
	fn void_fun<T, U, V>(&self, _: T, _: U, _: V) -> Result<()> { Ok(()) }

	fn max_clients_cc_fun(
		&self, msg: &s2c::InChannelCreatedPart, _: &mut Vec<Event>,
	) -> Result<(Option<MaxClients>, Option<MaxClients>)> {
		let ch = max_clients!(msg);
		let ch_fam = if msg.is_max_family_clients_unlimited {
			Some(MaxClients::Unlimited)
		} else if msg.inherits_max_family_clients.unwrap_or_default() {
			Some(MaxClients::Inherited)
		} else if msg.max_family_clients.map(|i| i >= 0 && i <= u16::MAX as i32).unwrap_or_default()
		{
			Some(MaxClients::Limited(msg.max_family_clients.unwrap() as u16))
		} else {
			// Max clients is less than zero or too high so ignore it
			None
		};
		Ok((ch, ch_fam))
	}
	fn max_clients_ce_fun(
		&mut self, channel_id: ChannelId, msg: &s2c::InChannelEditedPart, events: &mut Vec<Event>,
	) -> Result<()> {
		let channel = self.get_mut_channel(channel_id)?;

		let ch = max_clients!(msg);
		if let Some(ch) = ch {
			events.push(Event::PropertyChanged {
				id: PropertyId::ChannelMaxClients(channel_id),
				old: PropertyValue::OptionMaxClients(channel.max_clients.take()),
				invoker: msg.get_invoker(),
				extra: ExtraInfo { reason: Some(msg.reason) },
			});
			channel.max_clients = Some(ch);
		}
		let ch_fam = if msg.is_max_family_clients_unlimited.unwrap_or_default() {
			Some(MaxClients::Unlimited)
		} else if msg.inherits_max_family_clients.unwrap_or_default() {
			Some(MaxClients::Inherited)
		} else if msg.max_family_clients.map(|i| i >= 0 && i <= u16::MAX as i32).unwrap_or_default()
		{
			Some(MaxClients::Limited(msg.max_family_clients.unwrap() as u16))
		} else {
			// Max clients is less than zero or too high so ignore it
			None
		};
		if let Some(ch_fam) = ch_fam {
			events.push(Event::PropertyChanged {
				id: PropertyId::ChannelMaxFamilyClients(channel_id),
				old: PropertyValue::OptionMaxClients(channel.max_family_clients.take()),
				invoker: msg.get_invoker(),
				extra: ExtraInfo { reason: Some(msg.reason) },
			});
			channel.max_family_clients = Some(ch_fam);
		}
		Ok(())
	}
	fn max_clients_cl_fun(
		&self, msg: &s2c::InChannelListPart, _: &mut Vec<Event>,
	) -> Result<(Option<MaxClients>, Option<MaxClients>)> {
		let max_clients: i32 = msg.max_clients;
		let ch = if msg.is_max_clients_unlimited {
			Some(MaxClients::Unlimited)
		} else if max_clients >= 0 && max_clients <= u16::MAX as i32 {
			Some(MaxClients::Limited(max_clients as u16))
		} else {
			// Max clients is less than zero or too high so ignore it
			None
		};

		let max_clients: i32 = msg.max_family_clients;
		let ch_fam = if msg.is_max_family_clients_unlimited {
			Some(MaxClients::Unlimited)
		} else if msg.inherits_max_family_clients {
			Some(MaxClients::Inherited)
		} else if max_clients >= 0 && max_clients <= u16::MAX as i32 {
			Some(MaxClients::Limited(max_clients as u16))
		} else {
			// Max clients is less than zero or too high so ignore it
			Some(MaxClients::Unlimited)
		};
		Ok((ch, ch_fam))
	}

	fn channel_type_cc_fun(
		&self, msg: &s2c::InChannelCreatedPart, _: &mut Vec<Event>,
	) -> Result<ChannelType> {
		if msg.is_permanent.unwrap_or_default() {
			Ok(ChannelType::Permanent)
		} else if msg.is_semi_permanent.unwrap_or_default() {
			Ok(ChannelType::SemiPermanent)
		} else {
			Ok(ChannelType::Temporary)
		}
	}

	fn channel_type_ce_fun(
		&mut self, channel_id: ChannelId, msg: &s2c::InChannelEditedPart, events: &mut Vec<Event>,
	) -> Result<()> {
		let channel = self.get_mut_channel(channel_id)?;

		let typ = if let Some(perm) = msg.is_permanent {
			if perm { ChannelType::Permanent } else { ChannelType::Temporary }
		} else if msg.is_semi_permanent.unwrap_or_default() {
			ChannelType::SemiPermanent
		} else {
			return Ok(());
		};
		events.push(Event::PropertyChanged {
			id: PropertyId::ChannelChannelType(channel_id),
			old: PropertyValue::ChannelType(channel.channel_type),
			invoker: msg.get_invoker(),
			extra: ExtraInfo { reason: Some(msg.reason) },
		});
		channel.channel_type = typ;
		Ok(())
	}

	fn channel_type_cl_fun(
		&self, msg: &s2c::InChannelListPart, _: &mut Vec<Event>,
	) -> Result<ChannelType> {
		if msg.is_permanent {
			Ok(ChannelType::Permanent)
		} else if msg.is_semi_permanent {
			Ok(ChannelType::SemiPermanent)
		} else {
			Ok(ChannelType::Temporary)
		}
	}

	fn away_cev_fun(
		&self, msg: &s2c::InClientEnterViewPart, _: &mut Vec<Event>,
	) -> Result<Option<String>> {
		if msg.is_away { Ok(Some(msg.away_message.clone())) } else { Ok(None) }
	}

	fn client_type_cev_fun(
		&self, msg: &s2c::InClientEnterViewPart, _: &mut Vec<Event>,
	) -> Result<ClientType> {
		if msg.uid.is_server_admin() {
			if let ClientType::Query { .. } = msg.client_type {
				return Ok(ClientType::Query { admin: true });
			}
		}
		Ok(msg.client_type.clone())
	}

	fn away_cu_fun(
		&mut self, client_id: ClientId, msg: &s2c::InClientUpdatedPart, events: &mut Vec<Event>,
	) -> Result<()> {
		let client = self.get_mut_client(client_id)?;

		if let Some(is_away) = msg.is_away {
			if is_away != client.away_message.is_some() {
				let away = if is_away {
					Some(msg.away_message.clone().unwrap_or_else(String::new))
				} else {
					None
				};
				events.push(Event::PropertyChanged {
					id: PropertyId::ClientAwayMessage(client_id),
					old: PropertyValue::OptionString(client.away_message.take()),
					invoker: msg.get_invoker(),
					extra: ExtraInfo { reason: None },
				});
				client.away_message = away;
			}
		} else if let Some(away_message) = &msg.away_message {
			if let Some(cur_msg) = &client.away_message {
				if away_message != cur_msg {
					events.push(Event::PropertyChanged {
						id: PropertyId::ClientAwayMessage(client_id),
						old: PropertyValue::OptionString(client.away_message.take()),
						invoker: msg.get_invoker(),
						extra: ExtraInfo { reason: None },
					});
					client.away_message = Some(away_message.clone());
				}
			}
		}
		Ok(())
	}

	fn talk_power_cev_fun(
		&self, msg: &s2c::InClientEnterViewPart, _: &mut Vec<Event>,
	) -> Result<Option<TalkPowerRequest>> {
		if msg.talk_power_request_time.timestamp() > 0 {
			Ok(Some(TalkPowerRequest {
				time: msg.talk_power_request_time,
				message: msg.talk_power_request_message.clone(),
			}))
		} else {
			Ok(None)
		}
	}

	fn talk_power_cu_fun(
		&mut self, client_id: ClientId, msg: &s2c::InClientUpdatedPart, events: &mut Vec<Event>,
	) -> Result<()> {
		if let Some(talk_request) = msg.talk_power_request_time {
			let client = self.get_mut_client(client_id)?;

			let talk_request = if talk_request.timestamp() > 0 {
				Some(TalkPowerRequest {
					time: talk_request,
					message: msg.talk_power_request_message.clone().unwrap_or_else(String::new),
				})
			} else {
				None
			};
			events.push(Event::PropertyChanged {
				id: PropertyId::ClientTalkPowerRequest(client_id),
				old: PropertyValue::OptionTalkPowerRequest(client.talk_power_request.take()),
				invoker: msg.get_invoker(),
				extra: ExtraInfo { reason: None },
			});
			client.talk_power_request = talk_request;
		}
		Ok(())
	}

	fn address_fun(
		&self, msg: &s2c::InClientConnectionInfoPart, _: &mut Vec<Event>,
	) -> Result<Option<SocketAddr>> {
		if !msg.ip.is_empty() {
			Ok(Some(SocketAddr::new(msg.ip.parse().map_err(Error::InvalidConnectionIp)?, msg.port)))
		} else {
			Ok(None)
		}
	}

	fn channel_subscribe_fun(
		&mut self, channel_id: ChannelId, _: &s2c::InChannelSubscribedPart, events: &mut Vec<Event>,
	) -> Result<()> {
		let channel = self.get_mut_channel(channel_id)?;
		events.push(Event::PropertyChanged {
			id: PropertyId::ChannelSubscribed(channel_id),
			old: PropertyValue::Bool(channel.subscribed),
			invoker: None,
			extra: ExtraInfo { reason: None },
		});
		channel.subscribed = true;
		Ok(())
	}

	fn channel_unsubscribe_fun(
		&mut self, channel_id: ChannelId, _: &s2c::InChannelUnsubscribedPart,
		events: &mut Vec<Event>,
	) -> Result<()>
	{
		let channel = self.get_mut_channel(channel_id)?;
		events.push(Event::PropertyChanged {
			id: PropertyId::ChannelSubscribed(channel_id),
			old: PropertyValue::Bool(channel.subscribed),
			invoker: None,
			extra: ExtraInfo { reason: None },
		});
		channel.subscribed = false;

		// Remove all known clients from this channel
		let remove_clients = self
			.clients
			.values()
			.filter_map(|c| if c.channel == channel_id { Some(c.id) } else { None })
			.collect::<Vec<_>>();
		for id in remove_clients {
			events.push(Event::PropertyRemoved {
				id: PropertyId::Client(id),
				old: PropertyValue::Client(self.clients.remove(&id).unwrap()),
				invoker: None,
				extra: ExtraInfo { reason: None },
			});
		}
		Ok(())
	}

	fn channel_order_remove(
		&mut self, channel_id: ChannelId, channel_order: ChannelId, events: &mut Vec<Event>,
	) {
		// [ C:7 | O:_ ]
		// [ C:5 | O:7 ] ─>X
		// [ C:_ | O:5 ]     (Upd: O -> 7)
		self.channels.values_mut().any(|c| {
			if c.order == channel_id && c.id != channel_id {
				events.push(Event::PropertyChanged {
					id: PropertyId::ChannelOrder(c.id),
					old: PropertyValue::ChannelId(c.order),
					invoker: None,
					extra: ExtraInfo { reason: None },
				});
				c.order = channel_order;
				true
			} else {
				false
			}
		});
	}

	fn channel_order_insert(
		&mut self, channel_id: ChannelId, channel_order: ChannelId, channel_parent: ChannelId,
		events: &mut Vec<Event>,
	)
	{
		// [ C:7 | O:_ ]
		// [            <── (New: C:5 | O:7)
		// [ C:_ | O:7 ]    (Upd: O -> 5)
		//
		// Also work for the first channel, the order will be 0.
		self.channels.values_mut().any(|c| {
			if c.order == channel_order && c.parent == channel_parent && c.id != channel_id {
				events.push(Event::PropertyChanged {
					id: PropertyId::ChannelOrder(c.id),
					old: PropertyValue::ChannelId(c.order),
					invoker: None,
					extra: ExtraInfo { reason: None },
				});
				c.order = channel_id;
				true
			} else {
				false
			}
		});
	}

	fn channel_order_cc_fun(
		&mut self, msg: &s2c::InChannelCreatedPart, events: &mut Vec<Event>,
	) -> Result<ChannelId> {
		self.channel_order_insert(msg.channel_id, msg.order, msg.parent_id, events);
		Ok(msg.order)
	}

	fn channel_order_ce_fun(
		&mut self, channel_id: ChannelId, msg: &s2c::InChannelEditedPart, events: &mut Vec<Event>,
	) -> Result<()> {
		self.channel_order_move_fun(channel_id, msg.order, msg.parent_id, events)
	}

	fn channel_order_cm_fun(
		&mut self, channel_id: ChannelId, msg: &s2c::InChannelMovedPart, events: &mut Vec<Event>,
	) -> Result<()> {
		self.channel_order_move_fun(channel_id, Some(msg.order), Some(msg.parent_id), events)
	}

	fn channel_order_move_fun(
		&mut self, channel_id: ChannelId, new_order: Option<ChannelId>, parent: Option<ChannelId>,
		events: &mut Vec<Event>,
	) -> Result<()>
	{
		let old_order;
		let new_parent;
		{
			let channel = self.get_mut_channel(channel_id)?;
			old_order = channel.order;
			new_parent = parent.unwrap_or(channel.parent);
			if let Some(order) = new_order {
				events.push(Event::PropertyChanged {
					id: PropertyId::ChannelOrder(channel.id),
					old: PropertyValue::ChannelId(channel.order),
					invoker: None,
					extra: ExtraInfo { reason: None },
				});
				channel.order = order;
			}
		}
		self.channel_order_remove(channel_id, old_order, events);
		self.channel_order_insert(channel_id, new_order.unwrap_or(old_order), new_parent, events);
		Ok(())
	}

	// Book to messages
	fn away_fun_b2m<'a>(msg: Option<&'a str>) -> (bool, &'a str) {
		if let Some(msg) = msg { (true, msg) } else { (false, "") }
	}
}

impl Client {
	// Book to messages
	fn password_b2m<'a>(password: &'a str) -> &'a str { password }
	fn channel_id_b2m(&self, channel: ChannelId) -> ChannelId { channel }
}

impl Channel {
	// Book to messages
	fn password_flagged_b2m<'a>(password: Option<&'a str>) -> (bool, &'a str) {
		if let Some(password) = password { (true, password) } else { (false, "") }
	}

	fn channel_type_fun_b2m(channel_type: ChannelType) -> (bool, bool) {
		match channel_type {
			ChannelType::Temporary => (false, false),
			ChannelType::SemiPermanent => (true, false),
			ChannelType::Permanent => (false, true),
		}
	}

	fn max_clients_fun_b2m(max_clients: MaxClients) -> (i32, bool) {
		match max_clients {
			MaxClients::Inherited => (0, false),
			MaxClients::Unlimited => (0, true),
			MaxClients::Limited(num) => (num.into(), false),
		}
	}

	fn max_family_clients_fun_b2m(max_clients: MaxClients) -> (i32, bool, bool) {
		match max_clients {
			MaxClients::Inherited => (0, false, true),
			MaxClients::Unlimited => (0, true, false),
			MaxClients::Limited(num) => (num.into(), false, false),
		}
	}

	fn channel_id_b2m(&self, channel: ChannelId) -> ChannelId { channel }
}

// TODO ClientServerGroup?
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ClientServerGroup {
	database_id: ClientDbId,
	inner: ServerGroupId,
}
impl ClientServerGroup {
	fn get_id(&self) -> ServerGroupId { self.inner }
}

/// The `ChannelOptions` are used to set initial properties of a new channel.
///
/// A channel can be created with [`ServerMut::add_channel`]. The only necessary
/// property of a channel is the name, all other properties will be set to their
/// default value.
///
/// [`ServerMut::add_channel`]: struct.ServerMut.html#method.add_channel
#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ChannelOptions<'a> {
	name: &'a str,
	description: Option<&'a str>,
	parent_id: Option<ChannelId>,
	codec: Option<Codec>,
	codec_quality: Option<u8>,
	delete_delay: Option<Duration>,
	password: Option<&'a str>,
	is_default: bool,
	max_clients: Option<MaxClients>,
	max_family_clients: Option<MaxClients>,
	channel_type: Option<ChannelType>,
	is_unencrypted: Option<bool>,
	order: Option<ChannelId>,
	phonetic_name: Option<&'a str>,
	topic: Option<&'a str>,
}

impl<'a> ChannelOptions<'a> {
	/// Create new `ChannelOptions` to add a new channel to a server.
	///
	/// # Arguments
	/// You have to supply a name for the new channel. All other properties are
	/// optional.
	pub fn new(name: &'a str) -> Self {
		Self {
			name,
			description: None,
			parent_id: None,
			codec: None,
			codec_quality: None,
			delete_delay: None,
			password: None,
			is_default: false,
			max_clients: None,
			max_family_clients: None,
			channel_type: None,
			is_unencrypted: None,
			order: None,
			phonetic_name: None,
			topic: None,
		}
	}

	pub fn description(mut self, description: &'a str) -> Self {
		self.description = Some(description);
		self
	}

	pub fn parent_id(mut self, parent_id: ChannelId) -> Self {
		self.parent_id = Some(parent_id);
		self
	}

	pub fn codec(mut self, codec: Codec) -> Self {
		self.codec = Some(codec);
		self
	}

	pub fn codec_quality(mut self, codec_quality: u8) -> Self {
		self.codec_quality = Some(codec_quality);
		self
	}

	pub fn delete_delay(mut self, delete_delay: Duration) -> Self {
		self.delete_delay = Some(delete_delay);
		self
	}

	pub fn password(mut self, password: &'a str) -> Self {
		self.password = Some(password);
		self
	}

	pub fn default(mut self) -> Self {
		self.is_default = true;
		self
	}

	pub fn max_clients(mut self, max_clients: MaxClients) -> Self {
		self.max_clients = Some(max_clients);
		self
	}

	pub fn max_family_clients(mut self, max_family_clients: MaxClients) -> Self {
		self.max_family_clients = Some(max_family_clients);
		self
	}

	pub fn channel_type(mut self, channel_type: ChannelType) -> Self {
		self.channel_type = Some(channel_type);
		self
	}

	pub fn is_unencrypted(mut self, is_unencrypted: bool) -> Self {
		self.is_unencrypted = Some(is_unencrypted);
		self
	}

	/// The previous order
	pub fn order(mut self, order: ChannelId) -> Self {
		self.order = Some(order);
		self
	}

	pub fn phonetic_name(mut self, phonetic_name: &'a str) -> Self {
		self.phonetic_name = Some(phonetic_name);
		self
	}

	pub fn topic(mut self, topic: &'a str) -> Self {
		self.topic = Some(topic);
		self
	}
}

impl Server {
	pub fn add_channel(&self, options: ChannelOptions) -> OutCommand {
		let inherits_max_family_clients = options
			.max_family_clients
			.as_ref()
			.and_then(|m| if let MaxClients::Inherited = m { Some(true) } else { None });
		let is_max_family_clients_unlimited = options
			.max_family_clients
			.as_ref()
			.and_then(|m| if let MaxClients::Unlimited = m { Some(true) } else { None });
		let max_family_clients = options
			.max_family_clients
			.as_ref()
			.and_then(|m| if let MaxClients::Limited(n) = m { Some(*n as i32) } else { None });
		let is_max_clients_unlimited = options
			.max_clients
			.as_ref()
			.and_then(|m| if let MaxClients::Unlimited = m { Some(true) } else { None });
		let max_clients = options
			.max_clients
			.as_ref()
			.and_then(|m| if let MaxClients::Limited(n) = m { Some(*n as i32) } else { None });

		let is_permanent = options
			.channel_type
			.as_ref()
			.and_then(|t| if *t == ChannelType::Permanent { Some(true) } else { None });
		let is_semi_permanent = options
			.channel_type
			.as_ref()
			.and_then(|t| if *t == ChannelType::SemiPermanent { Some(true) } else { None });

		c2s::OutChannelCreateMessage::new(&mut iter::once(c2s::OutChannelCreatePart {
			name: options.name,
			description: options.description,
			parent_id: options.parent_id,
			codec: options.codec,
			codec_quality: options.codec_quality,
			delete_delay: options.delete_delay,
			has_password: if options.password.is_some() { Some(true) } else { None },
			is_default: if options.is_default { Some(true) } else { None },
			inherits_max_family_clients,
			is_max_family_clients_unlimited,
			is_max_clients_unlimited,
			is_permanent,
			is_semi_permanent,
			max_family_clients,
			max_clients,
			is_unencrypted: options.is_unencrypted,
			order: options.order,
			password: options.password,
			phonetic_name: options.phonetic_name,
			topic: options.topic,
		}))
	}

	pub fn send_textmessage(&self, message: &str) -> OutCommand {
		c2s::OutSendTextMessageMessage::new(&mut iter::once(c2s::OutSendTextMessagePart {
			target: TextMessageTargetMode::Server,
			target_client_id: None,
			message,
		}))
	}

	/// Subscribe or unsubscribe from all channels.
	pub fn set_subscribed(&self, subscribed: bool) -> OutCommand {
		if subscribed {
			c2s::OutChannelSubscribeAllMessage::new()
		} else {
			c2s::OutChannelUnsubscribeAllMessage::new()
		}
	}
}

impl Connection {
	pub fn send_message(&self, target: MessageTarget, message: &str) -> OutCommand {
		match target {
			MessageTarget::Server => {
				c2s::OutSendTextMessageMessage::new(&mut iter::once(c2s::OutSendTextMessagePart {
					target: TextMessageTargetMode::Server,
					target_client_id: None,
					message,
				}))
			}
			MessageTarget::Channel => {
				c2s::OutSendTextMessageMessage::new(&mut iter::once(c2s::OutSendTextMessagePart {
					target: TextMessageTargetMode::Channel,
					target_client_id: None,
					message,
				}))
			}
			MessageTarget::Client(id) => {
				c2s::OutSendTextMessageMessage::new(&mut iter::once(c2s::OutSendTextMessagePart {
					target: TextMessageTargetMode::Client,
					target_client_id: Some(id),
					message,
				}))
			}
			MessageTarget::Poke(id) => c2s::OutClientPokeRequestMessage::new(&mut iter::once(
				c2s::OutClientPokeRequestPart { client_id: id, message },
			)),
		}
	}

	pub fn disconnect(&self, options: crate::DisconnectOptions) -> OutCommand {
		c2s::OutDisconnectMessage::new(&mut iter::once(c2s::OutDisconnectPart {
			reason: options.reason,
			reason_message: options.message.as_ref().map(|m| m.as_str()),
		}))
	}
}

impl Client {
	// TODO Move other clients
	/*/// Move this client to another channel.
	/// This function takes a password so it is possible to join protected
	/// channels.
	///
	/// # Examples
	/// ```rust,no_run
	/// # use futures::Future;
	/// # let connection: tsclientlib::Connection = panic!();
	/// let con_lock = connection.lock();
	/// let con_mut = con_lock.to_mut();
	/// // Get our own client in mutable form
	/// let client = con_mut.get_server().get_client(&con_lock.own_client).unwrap();
	/// // Switch to channel 2
	/// tokio::spawn(client.set_channel_with_password(ChannelId(2), "secure password")
	///	    .map_err(|e| println!("Failed to switch channel ({:?})", e)));
	/// ```*/

	pub fn send_textmessage(&self, message: &str) -> OutCommand {
		c2s::OutSendTextMessageMessage::new(&mut iter::once(c2s::OutSendTextMessagePart {
			target: TextMessageTargetMode::Client,
			target_client_id: Some(self.id),
			message,
		}))
	}

	pub fn poke(&self, message: &str) -> OutCommand {
		c2s::OutClientPokeRequestMessage::new(&mut iter::once(c2s::OutClientPokeRequestPart {
			client_id: self.id,
			message,
		}))
	}
}
