#![allow(dead_code)]
use std::net::IpAddr;
use std::ops::Deref;

use anyhow::Result;
use ts_bookkeeping::data::*;
use ts_bookkeeping::*;

use crate::{ConnectedConnection, MessageHandle, MessageTarget};

include!(concat!(env!("OUT_DIR"), "/b2mdecls.rs"));
include!(concat!(env!("OUT_DIR"), "/facades.rs"));

impl ServerMut<'_> {
	/// Create a new channel.
	///
	/// # Arguments
	/// All initial properties of the channel can be set through the
	/// [`ChannelOptions`] argument.
	///
	/// # Examples
	/// ```no_run
	/// use tsclientlib::data::ChannelOptions;
	/// # use futures::prelude::*;
	/// # let connection: tsclientlib::Connection = panic!();
	/// let mut state = connection.get_mut_state().unwrap();
	/// let handle = state.get_server().add_channel(ChannelOptions::new("My new channel")).unwrap();
	/// ```
	///
	/// [`ChannelOptions`]: struct.ChannelOptions.html
	pub fn add_channel(&mut self, options: ChannelOptions) -> Result<MessageHandle> {
		self.connection.send_command(self.inner.add_channel(options))
	}

	/// Send a text message in the server chat.
	///
	/// # Examples
	/// ```no_run
	/// # use futures::prelude::*;
	/// # let connection: tsclientlib::Connection = panic!();
	/// let mut state = connection.get_mut_state().unwrap();
	/// let handle = state.get_server().send_textmessage("Hi").unwrap();
	/// ```
	pub fn send_textmessage(&mut self, message: &str) -> Result<MessageHandle> {
		self.connection.send_command(self.inner.send_textmessage(message))
	}

	/// Subscribe or unsubscribe from all channels.
	///
	/// # Examples
	/// ```no_run
	/// # use futures::prelude::*;
	/// # let connection: tsclientlib::Connection = panic!();
	/// let mut state = connection.get_mut_state().unwrap();
	/// let handle = state.get_server().set_subscribed(true).unwrap();
	/// ```
	pub fn set_subscribed(&mut self, subscribed: bool) -> Result<MessageHandle> {
		self.connection.send_command(self.inner.set_subscribed(subscribed))
	}
}

impl ConnectionMut<'_> {
	/// A generic method to send a text message or poke a client.
	///
	/// # Examples
	/// ```no_run
	/// # use futures::prelude::*;
	/// # use tsclientlib::MessageTarget;
	/// # let connection: tsclientlib::Connection = panic!();
	/// let mut state = connection.get_mut_state().unwrap();
	/// let handle = state.send_message(MessageTarget::Server, "Hi").unwrap();
	/// ```
	pub fn send_message(&mut self, target: MessageTarget, message: &str) -> Result<MessageHandle> {
		self.connection.send_command(self.inner.send_message(target, message))
	}
}

impl ClientMut<'_> {
	// TODO clientmove
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
	/// ```no_run
	/// # use futures::prelude::*;
	/// # let connection: tsclientlib::Connection = panic!();
	/// let mut state = connection.get_mut_state().unwrap();
	/// // Get our own client in mutable form
	/// let client = state.get_client(&state.own_client).unwrap();
	/// // Send a message
	/// let handle = client.send_textmessage("Hi me!").unwrap();
	/// ```
	pub fn send_textmessage(&mut self, message: &str) -> Result<MessageHandle> {
		self.connection.send_command(self.inner.send_textmessage(message))
	}

	/// Poke this client with a message.
	///
	/// # Examples
	/// ```no_run
	/// # use futures::prelude::*;
	/// # let connection: tsclientlib::Connection = panic!();
	/// let mut state = connection.get_mut_state().unwrap();
	/// // Get our own client in mutable form
	/// let client = state.get_client(&state.own_client).unwrap();
	/// // Send a message
	/// let handle = client.poke("Hihihi").unwrap();
	/// ```
	pub fn poke(&mut self, message: &str) -> Result<MessageHandle> {
		self.connection.send_command(self.inner.poke(message))
	}
}
