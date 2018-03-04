use std::cell::RefCell;
use std::net::SocketAddr;
use std::ops::{Deref, DerefMut};
use std::rc::{Rc, Weak};

use chrono::{DateTime, Duration, Utc};
use futures::{self, Stream};
use tsproto::Error as tsproto_error;
use tsproto::client;
use tsproto_commands::*;
use tsproto_commands::messages::*;

use {ChannelType, Map, MaxFamilyClients, TalkPowerRequest};

include!(concat!(env!("OUT_DIR"), "/structs.rs"));
include!(concat!(env!("OUT_DIR"), "/m2bdecls.rs"));

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
                ask_for_privilegekey: packet.ask_for_privilege,
                license: packet.license_type,

                optional_data: None,
                connection_data: None,
                clients: Map::new(),
                channels: Map::new(),
                groups: Map::new(),
            ),
        }
    }

    fn handle_message(&mut self, msg: &Notification) {
        // TODO Replace by code generation
        // Also raise events
        match *msg {
            _ => {} // TODO
        }
    }

    // Backing functions for MessageToBook declarations

    fn return_false<T>(&self, _: T) -> bool { false }
    fn return_none<T, O>(&self, _: T) -> Option<O> { None }

    fn max_clients_cc_fun(&self, cmd: &ChannelCreated) -> (Option<u16>, MaxFamilyClients) {
        let ch = if cmd.is_max_clients_unlimited { None } else { Some(cmd.max_clients) };
        let ch_fam =
            if cmd.is_max_family_clients_unlimited { MaxFamilyClients::Unlimited }
            else if cmd.inherits_max_family_clients { MaxFamilyClients::Inherited }
            else { MaxFamilyClients::Limited(cmd.max_family_clients) };
        (ch, ch_fam)
    }
    fn max_clients_ce_fun(&self, cmd: &ChannelEdited, _: &mut Channel) -> (Option<u16>, MaxFamilyClients) {
        let ch = if cmd.is_max_clients_unlimited { None } else { Some(cmd.max_clients) };
        let ch_fam =
            if cmd.is_max_family_clients_unlimited { MaxFamilyClients::Unlimited }
            else if cmd.inherits_max_family_clients { MaxFamilyClients::Inherited }
            else { MaxFamilyClients::Limited(cmd.max_family_clients) };
        (ch, ch_fam)
    }
    fn max_clients_cl_fun(&self, cmd: &ChannelList) -> (Option<u16>, MaxFamilyClients) {
        let ch = if cmd.is_max_clients_unlimited { None } else { Some(cmd.max_clients) };
        let ch_fam =
            if cmd.is_max_family_clients_unlimited { MaxFamilyClients::Unlimited }
            else if cmd.inherits_max_family_clients { MaxFamilyClients::Inherited }
            else { MaxFamilyClients::Limited(cmd.max_family_clients) };
        (ch, ch_fam)
    }

    fn channel_type_cc_fun(&self, cmd: &ChannelCreated) -> ChannelType {
        if cmd.is_permanent { ChannelType::Permanent }
        else if cmd.is_semi_permanent { ChannelType::SemiPermanent }
        else { ChannelType::Temporary }
    }

    fn channel_type_ce_fun(&self, cmd: &ChannelEdited, _: &mut Channel) -> ChannelType {
        if cmd.is_permanent { ChannelType::Permanent }
        else if cmd.is_semi_permanent { ChannelType::SemiPermanent }
        else { ChannelType::Temporary }
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
    pub inner_stream: Box<Stream<Item = Notification, Error = tsproto_error>>,
}

impl NetworkWrapper {
    pub fn new(
        id: ConnectionId,
        client_data: Rc<RefCell<client::ClientData>>,
        client_connection: Weak<RefCell<client::ClientConnection>>,
        inner_stream: Box<Stream<Item = Notification, Error = tsproto_error>>,
        initserver: InitServer,
    ) -> Self {
        let connection = Connection::new(id, Uid(String::from("TODO")),
            &initserver);
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
    type Item = Notification;
    type Error = tsproto_error;

    fn poll(&mut self) -> futures::Poll<Option<Self::Item>, Self::Error> {
        let res = self.inner_stream.poll()?;
        if let futures::Async::Ready(Some(ref msg)) = res {
            self.connection.handle_message(msg);
        }
        Ok(res)
    }
}
