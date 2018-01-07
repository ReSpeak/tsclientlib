//! The base class of bookkeeping is the [`Connection`].
//!
//! [`Connection`]:
extern crate chrono;
extern crate futures;
extern crate tsproto;
extern crate tsproto_commands;

use std::cell::RefCell;
use std::net::SocketAddr;
use std::rc::Rc;

use chrono::{DateTime, Duration, Utc};
use futures::Stream;

use tsproto::errors::Error;
use tsproto_commands::*;
use tsproto_commands::messages::*;

macro_rules! copy_attrs {
    ($from:ident, $to:ident; $($attr:ident),* $(,)*; $($extra:ident: $ex:expr),* $(,)*) => {
        $to {
            $($attr: $from.$attr.clone(),)*
            $($extra: $ex,)*
        }
    };
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum ChannelType {
    Permanent,
    SemiPermanent,
    Temporary,
}

include!(concat!(env!("OUT_DIR"), "/structs.rs"));

impl Connection {
    pub fn new(id: ConnectionId, server_uid: Uid, packet: &InitServer)
        -> Rc<RefCell<Self>> {
        Rc::new(RefCell::new(Self {
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
                clients: Vec::new(),
                channels: Vec::new(),
            ),
        }))
    }

    fn handle_message(connection: &Rc<RefCell<Self>>, msg: &Notification) {
        match *msg {
            Notification::ChannelList(ref packet) => {
                let mut con = connection.borrow_mut();
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

                    connection_id: con.id,
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
                con.server.channels.push(channel);
            }
            _ => unimplemented!(), // TODO
        }
    }
}

pub struct MessageHandlerStream;

impl MessageHandlerStream {
    pub fn new<Inner: Stream<Item = Notification, Error = Error> + 'static>(
        inner: Inner,
        connection: Rc<RefCell<Connection>>,
    ) -> Box<Stream<Item = Notification, Error = Error>> {
        Box::new(inner.map(move |msg| {
            Connection::handle_message(&connection, &msg);
            msg
        }))
    }
}
