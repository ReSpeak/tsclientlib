use std::fmt::Display;
use std::net::SocketAddr;

use futures::{future, Sink, Stream};
use slog::Logger;

use {Error, SinkWrapper, StreamWrapper};
use packets::{Packet, UdpPacket};

pub struct PacketLogger;
impl PacketLogger {
    fn prepare_logger<Id: Display>(
        logger: &Logger,
        id: &Id,
        is_client: bool,
        incoming: bool,
    ) -> Logger {
        let in_s = if incoming {
            if !cfg!(windows) {
                "\x1b[1;32mIN\x1b[0m"
            } else {
                "IN"
            }
        } else if !cfg!(windows) {
            "\x1b[1;31mOUT\x1b[0m"
        } else {
            "OUT"
        };
        let to_s = if is_client { "S" } else { "C" };
        let id_s = format!("{}", id);
        logger.new(o!("addr" => id_s, "dir" => in_s, "to" => to_s))
    }

    pub fn log_udp_packet(
        logger: &Logger,
        addr: SocketAddr,
        is_client: bool,
        incoming: bool,
        packet: &UdpPacket,
    ) {
        let logger = Self::prepare_logger(logger, &addr, is_client, incoming);
        debug!(logger, "UdpPacket"; "content" => ?packet);
    }

    pub fn log_packet<Id: Display>(
        logger: &Logger,
        id: &Id,
        is_client: bool,
        incoming: bool,
        packet: &Packet,
    ) {
        let logger = Self::prepare_logger(logger, id, is_client, incoming);
        debug!(logger, "Packet"; "content" => ?packet);
    }
}

pub struct PacketStreamLogger<Id> {
    phantom: ::std::marker::PhantomData<Id>,
}

impl<Inner: Stream<Item = (Id, Packet), Error = Error> + 'static,
    Id: Display + 'static>
    StreamWrapper<(Id, Packet), Error, Inner> for PacketStreamLogger<Id> {
    /// (logger, is_client)
    type A = (Logger, bool);
    type Result = Box<Stream<Item = (Id, Packet), Error = Error>>;

    fn wrap(inner: Inner, (logger, is_client): Self::A) -> Self::Result {
        Box::new(inner.inspect(move |&(ref id, ref packet)|
            PacketLogger::log_packet(&logger, id, is_client, true, packet)
        ))
    }
}

pub struct PacketSinkLogger<Id> {
    phantom: ::std::marker::PhantomData<Id>,
}

impl<Inner: Sink<SinkItem = (Id, Packet), SinkError = Error> + 'static,
    Id: Display + 'static>
    SinkWrapper<(Id, Packet), Error, Inner> for PacketSinkLogger<Id> {
    /// (logger, is_client)
    type A = (Logger, bool);
    type Result = Box<Sink<SinkItem = (Id, Packet), SinkError = Error>>;

    fn wrap(inner: Inner, (logger, is_client): Self::A) -> Self::Result {
        Box::new(inner.with(move |(id, packet)| {
            PacketLogger::log_packet(&logger, &id, is_client, false, &packet);
            future::ok((id, packet))
        }))
    }
}
