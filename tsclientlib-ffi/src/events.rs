//! Events for property changes
use std::os::raw::{c_char, c_void};

use num::{FromPrimitive, ToPrimitive};
use num_derive::{FromPrimitive, ToPrimitive};
use tsclientlib::{ChannelId, ClientId, Invoker, MaxClients, MessageTarget,
	ServerGroupId, TalkPowerRequest};

use tsclientlib::data::*;
use tsclientlib::events::{Event, PropertyId, PropertyValue, PropertyValueRef};

use crate::ffi_utils::ToFfi;
use crate::{EventContent, FfiInvoker, FfiMaxClients, FfiTalkPowerRequest,
	FfiResult, FfiResultType, NewEvent, NewFfiInvoker};

include!(concat!(env!("OUT_DIR"), "/events.rs"));
include!(concat!(env!("OUT_DIR"), "/ffigen.rs"));

unsafe impl Send for FfiProperty {}
unsafe impl Send for FfiPropertyId {}
unsafe impl Send for FfiPropertyValue {}

/// A property was added or removed.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct FfiProperty {
	pub value: FfiPropertyValue,
	/// If the name of the invoker is `null`, there is no invoker.
	pub invoker: FfiInvoker,
	pub p_type: FfiPropertyType,
	pub id: FfiPropertyId,
	/// When the value is an optional and it is `None`, this boolean is `false`.
	pub value_exists: bool,
}

/// A property was modified.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct FfiPropertyChanged {
	pub old: FfiPropertyValue,
	pub new: FfiPropertyValue,
	/// If the name of the invoker is `null`, there is no invoker.
	pub invoker: FfiInvoker,
	pub p_type: FfiPropertyType,
	pub id: FfiPropertyId,
	/// When the value is an optional and it is `None`, this boolean is `false`.
	pub old_exists: bool,
	/// When the value is an optional and it is `None`, this boolean is `false`.
	pub new_exists: bool,
}
