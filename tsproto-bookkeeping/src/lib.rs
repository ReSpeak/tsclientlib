extern crate chrono;
extern crate tsproto_commands;

pub mod structs;

use structs::*;

pub struct Bookkeeping {
    pub servers: Vec<Server>,
}
