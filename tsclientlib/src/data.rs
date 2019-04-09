#![allow(dead_code)] // TODO

use std::collections::HashMap;
use std::marker::PhantomData;
use std::mem;
use std::net::SocketAddr;
use std::ops::Deref;
use std::u16;

use chrono::{DateTime, Duration, Utc};
use failure::format_err;
use futures::Future;
use slog::{debug, Logger};
use tsproto_commands::messages::s2c::{self, InMessage, InMessages};
use tsproto_commands::*;

use crate::{Error, MessageTarget, Result};
use crate::events::{Event, PropertyValue, PropertyValueRef, PropertyId};

include!(concat!(env!("OUT_DIR"), "/b2mdecls.rs"));
include!(concat!(env!("OUT_DIR"), "/facades.rs"));
include!(concat!(env!("OUT_DIR"), "/m2bdecls.rs"));
include!(concat!(env!("OUT_DIR"), "/structs.rs"));
include!(concat!(env!("OUT_DIR"), "/properties.rs"));

macro_rules! max_clients {
	($cmd:ident) => {{
		if $cmd.is_max_clients_unlimited == Some(true) {
			Some(MaxClients::Unlimited)
		} else if $cmd.max_clients.map(|i| i >= 0 && i <= u16::MAX as i32).unwrap_or(false) {
			Some(MaxClients::Limited($cmd.max_clients.unwrap() as u16))
		} else {
			// Max clients is less than zero or too high so ignore it
			None
		}
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

	pub(crate) fn handle_message(&mut self, msg: &InMessage, logger: &Logger)
		-> Result<Vec<Event>> {
		// Returns if it handled the message so we can warn if a message is
		// unhandled.
		let (mut handled, mut events) = self.handle_message_generated(msg)?;
		// Handle special messages
		match msg.msg() {
			InMessages::TextMessage(cmd) => for cmd in cmd.iter() {
				let from = match cmd.target {
					TextMessageTargetMode::Server => MessageTarget::Server,
					TextMessageTargetMode::Channel => MessageTarget::Channel,
					TextMessageTargetMode::Client => MessageTarget::Client(cmd.invoker_id),
					TextMessageTargetMode::Unknown => return Err(format_err!(
						"Unknown TextMessageTargetMode").into()),
				};
				events.push(Event::Message {
					from,
					invoker: Invoker {
						name: cmd.invoker_name.into(),
						id: cmd.invoker_id,
						uid: cmd.invoker_uid.as_ref().map(|i| i.clone().into()),
					},
					message: cmd.message.to_string(),
				});
				handled = true;
			}
			InMessages::ClientPoke(cmd) => for cmd in cmd.iter() {
				events.push(Event::Message {
					from: MessageTarget::Poke(cmd.invoker_id),
					invoker: Invoker {
						name: cmd.invoker_name.into(),
						id: cmd.invoker_id,
						uid: cmd.invoker_uid.as_ref().map(|i| i.clone().into()),
					},
					message: cmd.message.to_string(),
				});
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
					});
				}
			}
		}

		if !handled {
			debug!(logger, "Unknown message"; "message" => msg.command().name());
		}

		Ok(events)
	}

	fn get_server(&self) -> &Server { &self.server }
	fn get_mut_server(&mut self) -> &mut Server { &mut self.server }

	fn get_server_group(&self, group: ServerGroupId) -> Result<&ServerGroup> {
		self.server.groups.get(&group)
			.ok_or_else(|| format_err!("ServerGroup {} not found", group).into())
	}
	fn add_server_group(
		&mut self,
		group: ServerGroupId,
		r: ServerGroup,
	) -> Option<ServerGroup>
	{
		self.server.groups.insert(group, r)
	}

	fn get_optional_server_data(&self) -> Result<&OptionalServerData> {
		self.server.optional_data.as_ref()
			.ok_or_else(|| format_err!("Server has no optional data").into())
	}

	fn get_connection_server_data(&self) -> Result<&ConnectionServerData> {
		self.server.connection_data.as_ref()
			.ok_or_else(|| format_err!("Server has no connection data").into())
	}

	fn get_connection(&self) -> &Connection { &self }

	fn get_client(&self, client: ClientId) -> Result<&Client> {
		self.server
			.clients
			.get(&client)
			.ok_or_else(|| format_err!("Client {} not found", client).into())
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

	fn get_connection_client_data(&self, client: ClientId) -> Result<&ConnectionClientData> {
		if let Some(c) = self.server.clients.get(&client) {
			c.connection_data.as_ref()
				.ok_or_else(|| format_err!("Client {} has no connection data", client).into())
		} else {
			Err(format_err!("Client {} not found", client).into())
		}
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

	fn get_optional_client_data(&self, client: ClientId) -> Result<&OptionalClientData> {
		if let Some(c) = self.server.clients.get(&client) {
			c.optional_data.as_ref()
				.ok_or_else(|| format_err!("Client {} has no optional data", client).into())
		} else {
			Err(format_err!("Client {} not found", client).into())
		}
	}

	fn get_channel(&self, channel: ChannelId) -> Result<&Channel> {
		self.server
			.channels
			.get(&channel)
			.ok_or_else(|| format_err!("Channel {} not found", channel).into())
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

	fn get_optional_channel_data(&self, channel: ChannelId) -> Result<&OptionalChannelData> {
		if let Some(c) = self.server.channels.get(&channel) {
			c.optional_data.as_ref()
				.ok_or_else(|| format_err!("Channel {} has no optional data", channel).into())
		} else {
			Err(format_err!("Channel {} not found", channel).into())
		}
	}

	fn get_file(&self, _channel: ChannelId, _path: &str, _name: &str) -> Result<&File> {
		unimplemented!("Files are not yet implemented")
	}

	fn get_chat_entry(&self, _sender: ClientId) -> Result<&ChatEntry> {
		unimplemented!("Files are not yet implemented")
	}

	// Backing functions for MessageToBook declarations

	fn return_false<T>(&self, _: T) -> bool { false }
	fn return_none<T, O>(&self, _: T) -> Option<O> { None }
	fn void_fun<T, U, V>(&self, _: T, _: U, _: V) {}
	fn return_some<T>(&self, t: T) -> Option<T> { Some(t) }

	fn max_clients_cc_fun(
		&self,
		cmd: &s2c::ChannelCreatedPart,
	) -> (Option<MaxClients>, Option<MaxClients>)
	{
		let ch = max_clients!(cmd);
		let ch_fam = if cmd.is_max_family_clients_unlimited {
			Some(MaxClients::Unlimited)
		} else if cmd.inherits_max_family_clients == Some(true) {
			Some(MaxClients::Inherited)
		} else if cmd.max_family_clients.map(|i| i >= 0 && i <= u16::MAX as i32).unwrap_or(false) {
			Some(MaxClients::Limited(cmd.max_family_clients.unwrap() as u16))
		} else {
			// Max clients is less than zero or too high so ignore it
			None
		};
		(ch, ch_fam)
	}
	fn max_clients_ce_fun(
		&mut self,
		channel_id: ChannelId,
		cmd: &s2c::ChannelEditedPart,
		events: &mut Vec<Event>,
	)
	{
		if let Ok(channel) = self.get_mut_channel(channel_id) {
			let invoker = if let Some(id) = cmd.invoker_id {
				if let Some(name) = cmd.invoker_name {
					Some(Invoker {
						id,
						name: name.to_string(),
						uid: cmd.invoker_uid.as_ref().map(|i| i.clone().into()),
					})
				} else {
					None
				}
			} else {
				None
			};

			let ch = max_clients!(cmd);
			if let Some(ch) = ch {
				events.push(Event::PropertyChanged {
					id: PropertyId::ChannelMaxClients(channel_id),
					old: PropertyValue::OptionMaxClients(channel.max_clients.take()),
					invoker: invoker.clone(),
				});
				channel.max_clients = Some(ch);
			}
			let ch_fam = if cmd.is_max_family_clients_unlimited == Some(true) {
				Some(MaxClients::Unlimited)
			} else if cmd.inherits_max_family_clients == Some(true) {
				Some(MaxClients::Inherited)
			} else if cmd.max_family_clients.map(|i| i >= 0 && i <= u16::MAX as i32).unwrap_or(false) {
				Some(MaxClients::Limited(cmd.max_family_clients.unwrap() as u16))
			} else {
				// Max clients is less than zero or too high so ignore it
				None
			};
			if let Some(ch_fam) = ch_fam {
				events.push(Event::PropertyChanged {
					id: PropertyId::ChannelMaxFamilyClients(channel_id),
					old: PropertyValue::OptionMaxClients(channel.max_family_clients.take()),
					invoker,
				});
				channel.max_family_clients = Some(ch_fam);
			}
		}
	}
	fn max_clients_cl_fun(
		&self,
		cmd: &s2c::ChannelListPart,
	) -> (Option<MaxClients>, Option<MaxClients>)
	{
		let ch = if cmd.is_max_clients_unlimited {
			Some(MaxClients::Unlimited)
		} else if cmd.max_clients >= 0 && cmd.max_clients <= u16::MAX as i32 {
			Some(MaxClients::Limited(cmd.max_clients as u16))
		} else {
			// Max clients is less than zero or too high so ignore it
			None
		};
		let ch_fam = if cmd.is_max_family_clients_unlimited {
			Some(MaxClients::Unlimited)
		} else if cmd.inherits_max_family_clients {
			Some(MaxClients::Inherited)
		} else if cmd.max_family_clients >= 0
			&& cmd.max_family_clients <= u16::MAX as i32
		{
			Some(MaxClients::Limited(cmd.max_family_clients as u16))
		} else {
			// Max clients is less than zero or too high so ignore it
			Some(MaxClients::Unlimited)
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
		events: &mut Vec<Event>,
	)
	{
		if let Ok(channel) = self.get_mut_channel(channel_id) {
			let invoker = if let Some(id) = cmd.invoker_id {
				if let Some(name) = cmd.invoker_name {
					Some(Invoker {
						id,
						name: name.to_string(),
						uid: cmd.invoker_uid.clone().map(|i| i.clone().into()),
					})
				} else {
					None
				}
			} else {
				None
			};

			let typ = if let Some(perm) = cmd.is_permanent {
				if perm {
					ChannelType::Permanent
				} else {
					ChannelType::Temporary
				}
			} else if cmd.is_semi_permanent == Some(true) {
				ChannelType::SemiPermanent
			} else {
				return;
			};
			events.push(Event::PropertyChanged {
				id: PropertyId::ChannelChannelType(channel_id),
				old: PropertyValue::ChannelType(channel.channel_type),
				invoker,
			});
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

	fn channel_subscribe_fun(
		&mut self,
		channel_id: ChannelId,
		_: &s2c::ChannelSubscribedPart,
		events: &mut Vec<Event>,
	) {
		if let Ok(channel) = self.get_mut_channel(channel_id) {
			events.push(Event::PropertyChanged {
				id: PropertyId::ChannelSubscribed(channel_id),
				old: PropertyValue::Bool(channel.subscribed),
				invoker: None,
			});
			channel.subscribed = true;
		}
	}

	fn channel_unsubscribe_fun(
		&mut self,
		channel_id: ChannelId,
		_: &s2c::ChannelUnsubscribedPart,
		events: &mut Vec<Event>,
	) {
		if let Ok(channel) = self.get_mut_channel(channel_id) {
			events.push(Event::PropertyChanged {
				id: PropertyId::ChannelSubscribed(channel_id),
				old: PropertyValue::Bool(channel.subscribed),
				invoker: None,
			});
			channel.subscribed = false;

			// Remove all known clients from this channel
			let server = self.get_mut_server();
			let remove_clients = server.clients.values().filter_map(|c|
				if c.channel == channel_id {
					Some(c.id)
				} else {
					None
				}).collect::<Vec<_>>();
			for id in remove_clients {
				events.push(Event::PropertyRemoved {
					id: PropertyId::Client(id),
					old: PropertyValue::Client(server.clients.remove(&id).unwrap()),
					invoker: None,
				});
			}
		}
	}


	// Book to messages
	fn away_fun_b2m<'a>(&self, msg: Option<&'a str>) -> (Option<bool>, Option<&'a str>) {
		(Some(msg.is_some()), msg)
	}

}

impl Client {
	// Book to messages
	fn get_empty_string(&self) -> &str { "" }
	fn identity<T>(&self, t: T) -> T { t }
	fn return_some<T>(&self, t: T) -> Option<T> { Some(t) }
}

impl ClientServerGroupMut<'_> {
	fn get_id(&self) -> ServerGroupId { *self.inner }
}

/// The `ChannelOptions` are used to set initial properties of a new channel.
///
/// A channel can be created with [`ServerMut::add_channel`]. The only necessary
/// property of a channel is the name, all other properties will be set to their
/// default value.
///
/// [`ServerMut::add_channel`]: struct.ServerMut.html#method.add_channel
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
	order: Option<i32>,
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

	pub fn order(mut self, order: i32) -> Self {
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

impl ServerMut<'_> {
	/// Create a new channel.
	///
	/// # Arguments
	/// All initial properties of the channel can be set through the
	/// [`ChannelOptions`] argument.
	///
	/// # Examples
	/// ```rust,no_run
	/// use tsclientlib::data::ChannelOptions;
	/// # use futures::Future;
	/// # let connection: tsclientlib::Connection = panic!();
	///
	/// let con_lock = connection.lock();
	/// let con_mut = con_lock.to_mut();
	/// // Send a message
	/// tokio::spawn(con_mut.get_server().add_channel(ChannelOptions::new("My new channel"))
	///	    .map_err(|e| println!("Failed to create channel ({:?})", e)));
	/// ```
	///
	/// [`ChannelOptions`]: struct.ChannelOptions.html
	#[must_use = "futures do nothing unless polled"]
	pub fn add_channel(&self, options: ChannelOptions) -> impl Future<Item=(), Error=Error> {
		let inherits_max_family_clients = options.max_family_clients.as_ref()
			.and_then(|m| if let MaxClients::Inherited = m { Some(true) } else { None });
		let is_max_family_clients_unlimited = options.max_family_clients.as_ref()
			.and_then(|m| if let MaxClients::Unlimited = m { Some(true) } else { None });
		let max_family_clients = options.max_family_clients.as_ref()
			.and_then(|m| if let MaxClients::Limited(n) = m { Some(*n as i32) } else { None });
		let is_max_clients_unlimited = options.max_clients.as_ref()
			.and_then(|m| if let MaxClients::Unlimited = m { Some(true) } else { None });
		let max_clients = options.max_clients.as_ref()
			.and_then(|m| if let MaxClients::Limited(n) = m { Some(*n as i32) } else { None });

		let is_permanent = options.channel_type.as_ref()
			.and_then(|t| if *t == ChannelType::Permanent { Some(true) } else { None });
		let is_semi_permanent = options.channel_type.as_ref()
			.and_then(|t| if *t == ChannelType::SemiPermanent { Some(true) } else { None });

		self.connection.send_packet(messages::c2s::OutChannelCreateMessage::new(
			vec![messages::c2s::ChannelCreatePart {
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
				phantom: PhantomData,
			}].into_iter()))
	}

	/// Send a text message in the server chat.
	///
	/// # Examples
	/// ```rust,no_run
	/// # use futures::Future;
	/// # let connection: tsclientlib::Connection = panic!();
	/// let con_lock = connection.lock();
	/// let con_mut = con_lock.to_mut();
	/// // Send a message
	/// tokio::spawn(con_mut.get_server().send_textmessage("Hi")
	///	    .map_err(|e| println!("Failed to send text message ({:?})", e)));
	/// ```
	#[must_use = "futures do nothing unless polled"]
	pub fn send_textmessage(&self, message: &str) -> impl Future<Item=(), Error=Error> {
		self.connection.send_packet(messages::c2s::OutSendTextMessageMessage::new(
			vec![messages::c2s::SendTextMessagePart {
				target: TextMessageTargetMode::Server,
				target_client_id: None,
				message,
				phantom: PhantomData,
			}].into_iter()))
	}

	/// Subscribe or unsubscribe from all channels.
	pub fn set_subscribed(&self, subscribed: bool) -> impl Future<Item=(), Error=Error> {
		if subscribed {
			self.connection.send_packet(messages::c2s::OutChannelSubscribeAllMessage::new(
				vec![messages::c2s::ChannelSubscribeAllPart {
					phantom: PhantomData,
				}].into_iter()))
		} else {
			self.connection.send_packet(messages::c2s::OutChannelUnsubscribeAllMessage::new(
				vec![messages::c2s::ChannelUnsubscribeAllPart {
					phantom: PhantomData,
				}].into_iter()))
		}
	}
}

impl ConnectionMut<'_> {
	/// A generic method to send a text message or poke a client.
	///
	/// # Examples
	/// ```rust,no_run
	/// # use futures::Future;
	/// # let connection: tsclientlib::Connection = panic!();
	/// let con_lock = connection.lock();
	/// let con_mut = con_lock.to_mut();
	/// // Send a message
	/// tokio::spawn(con_mut.send_message("Hi")
	///	    .map_err(|e| println!("Failed to send message ({:?})", e)));
	/// ```
	#[must_use = "futures do nothing unless polled"]
	pub fn send_message(&self, target: MessageTarget, message: &str) -> impl Future<Item=(), Error=Error> {
		match target {
			MessageTarget::Server => self.connection.send_packet(
				messages::c2s::OutSendTextMessageMessage::new(
					vec![messages::c2s::SendTextMessagePart {
						target: TextMessageTargetMode::Server,
						target_client_id: None,
						message,
						phantom: PhantomData,
					}].into_iter())),
			MessageTarget::Channel => self.connection.send_packet(
				messages::c2s::OutSendTextMessageMessage::new(
					vec![messages::c2s::SendTextMessagePart {
						target: TextMessageTargetMode::Channel,
						target_client_id: None,
						message,
						phantom: PhantomData,
					}].into_iter())),
			MessageTarget::Client(id) => self.connection.send_packet(
				messages::c2s::OutSendTextMessageMessage::new(
					vec![messages::c2s::SendTextMessagePart {
						target: TextMessageTargetMode::Client,
						target_client_id: Some(id),
						message,
						phantom: PhantomData,
					}].into_iter())),
			MessageTarget::Poke(id) => self.connection.send_packet(
				messages::c2s::OutClientPokeRequestMessage::new(
					vec![messages::c2s::ClientPokeRequestPart {
						client_id: id,
						message,
						phantom: PhantomData,
					}].into_iter())),
		}
	}
}

impl ClientMut<'_> {
	// TODO
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

	/// Send a text message to this client.
	///
	/// # Examples
	/// Greet a user:
	/// ```rust,no_run
	/// # use futures::Future;
	/// # let connection: tsclientlib::Connection = panic!();
	/// let con_lock = connection.lock();
	/// let con_mut = con_lock.to_mut();
	/// // Get our own client in mutable form
	/// let client = con_mut.get_server().get_client(&con_lock.own_client).unwrap();
	/// // Send a message
	/// tokio::spawn(client.send_textmessage("Hi me!")
	///	    .map_err(|e| println!("Failed to send me a text message ({:?})", e)));
	/// ```
	#[must_use = "futures do nothing unless polled"]
	pub fn send_textmessage(&self, message: &str) -> impl Future<Item=(), Error=Error> {
		self.connection.send_packet(messages::c2s::OutSendTextMessageMessage::new(
			vec![messages::c2s::SendTextMessagePart {
				target: TextMessageTargetMode::Client,
				target_client_id: Some(self.inner.id),
				message,
				phantom: PhantomData,
			}].into_iter()))
	}

	/// Poke this client with a message.
	///
	/// # Examples
	/// ```rust,no_run
	/// # use futures::Future;
	/// # let connection: tsclientlib::Connection = panic!();
	/// let con_lock = connection.lock();
	/// let con_mut = con_lock.to_mut();
	/// // Get our own client in mutable form
	/// let client = con_mut.get_server().get_client(&con_lock.own_client).unwrap();
	/// // Send a message
	/// tokio::spawn(client.poke("Hihihi")
	///	    .map_err(|e| println!("Failed to poke me ({:?})", e)));
	/// ```
	#[must_use = "futures do nothing unless polled"]
	pub fn poke(&self, message: &str) -> impl Future<Item=(), Error=Error> {
		self.connection.send_packet(messages::c2s::OutClientPokeRequestMessage::new(
			vec![messages::c2s::ClientPokeRequestPart {
				client_id: self.inner.id,
				message,
				phantom: PhantomData,
			}].into_iter()))
	}
}
