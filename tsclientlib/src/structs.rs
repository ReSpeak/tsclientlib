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

use {ChannelType, Map};

include!(concat!(env!("OUT_DIR"), "/structs.rs"));

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
                ;

                connection_id: id,
                uid: server_uid,
                name: packet.server_name.clone(),
                platform: packet.server_platform.clone(),
                version: packet.server_version.clone(),
                created: packet.server_created,
                icon: packet.icon_id,
                ip: packet.server_ip.clone(),
                ask_for_privilegekey: packet.ask_for_privilege,
                temp_channel_default_delete_delay:
                    packet.default_temp_channel_delete_delay,
                license: packet.license_type,

                optional_data: None,
                connection_data: None,
                clients: Map::new(),
                channels: Map::new(),
            ),
        }
    }

    fn handle_message(&mut self, msg: &Notification) {
        match *msg {
            Notification::ChannelList(ref packet) => {
                // Add new channel
                let channel = copy_attrs!(packet, Channel;
                    name,
                    topic,
                    codec,
                    codec_quality,
                    order,
                    has_password,
                    codec_latency_factor,
                    delete_delay,
                    needed_talk_power,
                    forced_silence,
                    phonetic_name,
                    is_unencrypted,
                    is_private,
                    ;

                    connection_id: self.id,
                    id: packet.channel_id,
                    parent: packet.channel_parent_id,
                    max_clients: if packet.is_max_clients_unlimited {
                        None
                    } else {
                        Some(packet.max_clients)
                    },
                    max_family_clients: if packet.is_max_family_clients_unlimited {
                        None
                    } else {
                        Some(packet.max_family_clients)
                    },
                    channel_type: if packet.is_permanent {
                        ChannelType::Permanent
                    } else if packet.is_semi_permanent {
                        ChannelType::SemiPermanent
                    } else {
                        ChannelType::Temporary
                    },
                    default: packet.is_default_channel,
                    icon: packet.icon_id,

                    optional_data: None,
                );
                self.server.channels.insert(channel.id, channel);
            }
            _ => {} // TODO
        }
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
