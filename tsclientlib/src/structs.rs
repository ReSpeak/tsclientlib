use std::cell::RefCell;
use std::mem;
use std::net::SocketAddr;
use std::ops::{Deref, DerefMut};
use std::rc::{Rc, Weak};
use std::u16;

use chrono::{DateTime, Duration, Utc};
use futures::{self, Stream};
use tsproto::Error as tsproto_error;
use tsproto::client;
use tsproto_commands::*;
use tsproto_commands::messages::*;

use {ChannelType, Map, MaxFamilyClients, TalkPowerRequest, Result};
use codec::Message;

include!(concat!(env!("OUT_DIR"), "/structs.rs"));
include!(concat!(env!("OUT_DIR"), "/m2bdecls.rs"));

macro_rules! max_clients {
    ($cmd:ident) => {{
        let ch = if $cmd.is_max_clients_unlimited { None }
            else if $cmd.max_clients >= 0 && $cmd.max_clients <= u16::MAX as i32 { Some($cmd.max_clients as u16) }
            else {
                // TODO Warning
                None
            };
        let ch_fam =
            if $cmd.is_max_family_clients_unlimited { MaxFamilyClients::Unlimited }
            else if $cmd.inherits_max_family_clients { MaxFamilyClients::Inherited }
            else if $cmd.max_family_clients >= 0 && $cmd.max_family_clients <= u16::MAX as i32 { MaxFamilyClients::Limited($cmd.max_family_clients as u16) }
            else {
                // TODO Warning
                MaxFamilyClients::Unlimited
            };
        (ch, ch_fam)
    }};
}

impl Connection {
    fn new(id: ConnectionId, server_uid: Uid, packet: &InitServer)
        -> Self {
        Self {
            id,
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

                connection_id: id,
                uid: server_uid,
                name: packet.name.clone(),
                platform: packet.server_platform.clone(),
                version: packet.server_version.clone(),
                created: packet.server_created,
                ip: packet.server_ip.clone(),
                ask_for_privilegekey: packet.ask_for_privilegekey,
                // TODO license: packet.license_type,
                license: LicenseType::NoLicense,

                optional_data: None,
                connection_data: None,
                clients: Map::new(),
                channels: Map::new(),
                groups: Map::new(),
            ),
        }
    }

    fn handle_message(&mut self, msg: &Message) -> Result<()> {
        if let Message::Message(ref notification) = *msg {
            self.handle_message_generated(&*notification)?;
        }

        // Also raise events
        match *msg {
            _ => {} // TODO
        }
        Ok(())
    }

    fn get_mut_server(&mut self) -> &mut Server { &mut self.server }
    fn add_server_group(&mut self, group: ServerGroupId, r: ServerGroup) -> Option<ServerGroup> { self.server.groups.insert(group, r) }

    fn get_mut_client(&mut self, client: ClientId) -> Result<&mut Client> {
        self.server.clients.get_mut(&client).ok_or_else(||
            format_err!("Client {} not found", client).into())
    }
    fn add_client(&mut self, client: ClientId, r: Client) -> Option<Client> { self.server.clients.insert(client, r) }
    fn remove_client(&mut self, client: ClientId) -> Option<Client> { self.server.clients.remove(&client) }
    fn add_connection_client_data(&mut self, client: ClientId, r: ConnectionClientData) -> Result<Option<ConnectionClientData>> {
        if let Some(client) = self.server.clients.get_mut(&client) {
            Ok(mem::replace(&mut client.connection_data, Some(r)))
        } else {
            Err(format_err!("Client {} not found", client).into())
        }
    }

    fn get_mut_channel(&mut self, channel: ChannelId) -> Result<&mut Channel> {
        self.server.channels.get_mut(&channel).ok_or_else(||
            format_err!("Channel {} not found", channel).into())
    }
    fn add_channel(&mut self, channel: ChannelId, r: Channel) -> Option<Channel> { self.server.channels.insert(channel, r) }
    fn remove_channel(&mut self, channel: ChannelId) -> Option<Channel> { self.server.channels.remove(&channel) }

    // Backing functions for MessageToBook declarations

    fn return_false<T>(&self, _: T) -> bool { false }
    fn return_none<T, O>(&self, _: T) -> Option<O> { None }
    fn void_fun<T, U>(&self, _: T, _: U) {}

    fn max_clients_cc_fun(&self, cmd: &ChannelCreated) -> (Option<u16>, MaxFamilyClients) {
        max_clients!(cmd)
    }
    fn max_clients_ce_fun(&mut self, channel_id: ChannelId, cmd: &ChannelEdited) {
        if let Ok(channel) = self.get_mut_channel(channel_id) {
            let (ch, ch_fam) = max_clients!(cmd);
            channel.max_clients = ch;
            channel.max_family_clients = ch_fam;
        }
    }
    fn max_clients_cl_fun(&self, cmd: &ChannelList) -> (Option<u16>, MaxFamilyClients) {
        max_clients!(cmd)
    }

    fn channel_type_cc_fun(&self, cmd: &ChannelCreated) -> ChannelType {
        if cmd.is_permanent { ChannelType::Permanent }
        else if cmd.is_semi_permanent { ChannelType::SemiPermanent }
        else { ChannelType::Temporary }
    }

    fn channel_type_ce_fun(&mut self, channel_id: ChannelId, cmd: &ChannelEdited) {
        if let Ok(channel) = self.get_mut_channel(channel_id) {
            let typ = if cmd.is_permanent { ChannelType::Permanent }
            else if cmd.is_semi_permanent { ChannelType::SemiPermanent }
            else { ChannelType::Temporary };
            channel.channel_type = typ;
        }
    }

    fn channel_type_cl_fun(&self, cmd: &ChannelList) -> ChannelType {
        if cmd.is_permanent { ChannelType::Permanent }
        else if cmd.is_semi_permanent { ChannelType::SemiPermanent }
        else { ChannelType::Temporary }
    }

    fn away_fun(&self, cmd: &ClientEnterView) -> Option<String> {
        if cmd.is_away { Some(cmd.away_message.clone()) }
        else { None }
    }

    fn talk_power_fun(&self, cmd: &ClientEnterView) -> Option<TalkPowerRequest> {
        // TODO optional time && msg
        if cmd.talk_power_request_time.timestamp() > 0 {
            Some( TalkPowerRequest {
                time: cmd.talk_power_request_time,
                message: cmd.talk_power_request_message.clone(),
            })
        } else {
            None
        }
    }

    fn badges_fun(&self, _cmd: &ClientEnterView) -> Vec<String> {
        Vec::new() // TODO
    }

    fn address_fun(&self, cmd: &ConnectionInfo) -> Option<SocketAddr> {
        let ip = if let Ok(ip) = cmd.ip.parse() { ip } else { return None };
        Some(SocketAddr::new(ip, cmd.port))
    }
}

pub struct NetworkWrapper {
    connection: Connection,
    pub client_data: Rc<RefCell<client::ClientData>>,
    pub client_connection: Weak<RefCell<client::ClientConnection>>,
    pub inner_stream: Box<Stream<Item = (SocketAddr, Message),
        Error = tsproto_error>>,
}

impl NetworkWrapper {
    pub fn new(
        id: ConnectionId,
        client_data: Rc<RefCell<client::ClientData>>,
        client_connection: Weak<RefCell<client::ClientConnection>>,
        initserver: &InitServer,
    ) -> Self {
        let connection = Connection::new(id, Uid(String::from("TODO")),
            initserver);
        let inner_stream = ::codec::CommandCodec::new_stream(&client_data);
        Self {
            connection,
            client_data,
            client_connection,
            inner_stream,
        }
    }
}

impl Deref for NetworkWrapper {
    type Target = Connection;

    fn deref(&self) -> &Self::Target {
        &self.connection
    }
}

impl DerefMut for NetworkWrapper {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.connection
    }
}

impl Stream for NetworkWrapper {
    type Item = (SocketAddr, Message);
    type Error = tsproto_error;

    fn poll(&mut self) -> futures::Poll<Option<Self::Item>, Self::Error> {
        let res = self.inner_stream.poll()?;
        if let futures::Async::Ready(Some((_, ref msg))) = res {
            if let Err(error) = self.connection.handle_message(msg) {
                warn!(self.client_data.borrow().logger,
                    "Error when handling message"; "error" => ?error);
            }
        }
        Ok(res)
    }
}
