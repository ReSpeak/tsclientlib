#![allow(dead_code)] // TODO

use std::collections::HashMap;
use std::marker::PhantomData;
use std::mem;
use std::net::SocketAddr;
use std::ops::Deref;
use std::u16;

use chrono::{DateTime, Duration, Utc};
use futures::Future;
use slog::{debug, Logger};
use tsproto_commands::messages::s2c::{self, InMessage, InMessages};
use tsproto_commands::*;

use crate::{Error, MessageTarget, Result};
use crate::events::{Event, Property, PropertyId};

include!(concat!(env!("OUT_DIR"), "/b2mdecls.rs"));
include!(concat!(env!("OUT_DIR"), "/facades.rs"));
include!(concat!(env!("OUT_DIR"), "/m2bdecls.rs"));
include!(concat!(env!("OUT_DIR"), "/structs.rs"));

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
						uid: Some(cmd.invoker_uid.clone().into()),
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
						uid: Some(cmd.invoker_uid.clone().into()),
					},
					message: cmd.message.to_string(),
				});
				handled = true;
			}
			_ => {}
		}

		if !handled {
			debug!(logger, "Unknown message"; "message" => msg.command().name());
		}

		Ok(events)
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
		} else if cmd.inherits_max_family_clients {
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
			let ch = max_clients!(cmd);
			if let Some(ch) = ch {
				events.push(Event::PropertyChanged(
					PropertyId::ChannelMaxClients(channel_id),
					Property::ChannelMaxClients(channel.max_clients.take()),
				));
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
				events.push(Event::PropertyChanged(
					PropertyId::ChannelMaxFamilyClients(channel_id),
					Property::ChannelMaxFamilyClients(channel.max_family_clients.take()),
				));
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
			events.push(Event::PropertyChanged(
				PropertyId::ChannelChannelType(channel_id),
				Property::ChannelChannelType(channel.channel_type),
			));
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
			events.push(Event::PropertyChanged(
				PropertyId::ChannelSubscribed(channel_id),
				Property::ChannelSubscribed(channel.subscribed),
			));
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
			events.push(Event::PropertyChanged(
				PropertyId::ChannelSubscribed(channel_id),
				Property::ChannelSubscribed(channel.subscribed),
			));
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
				events.push(Event::PropertyRemoved(
					PropertyId::Client(id),
					Property::Client(server.clients.remove(&id).unwrap()),
				));
			}
		}
	}


	// Book to messages
	fn away_fun_b2m<'a>(&self, msg: Option<&'a str>) -> (Option<bool>, Option<&'a str>) {
		(Some(msg.is_some()), msg)
	}

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
}

impl<'a> ChannelOptions<'a> {
	/// Create new `ChannelOptions` to add a new channel to a server.
	///
	/// # Arguments
	/// You have to supply a name for the new channel. All other properties are
	/// optional.
	pub fn new(name: &'a str) -> Self { Self { name } }
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
		self.connection.send_packet(messages::c2s::OutChannelCreateMessage::new(
			vec![messages::c2s::ChannelCreatePart {
				name: options.name,
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
