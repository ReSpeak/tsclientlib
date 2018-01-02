use chrono::{DateTime, Duration, Utc};

use tsproto_commands::*;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum ChannelType {
    Permanent,
    SemiPermanent,
    Temporary,
}

include!(concat!(env!("OUT_DIR"), "/structs.rs"));
