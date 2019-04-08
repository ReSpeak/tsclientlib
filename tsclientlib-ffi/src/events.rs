//! Events for property changes

use std::os::raw::{c_char, c_void};

use crate::{FfiMaxClients, FfiTalkPowerRequest, Invoker};

include!(concat!(env!("OUT_DIR"), "/events.rs"));

/// A property was added or removed.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct FfiProperty {
	/// If the name of the invoker is `null`, there is no invoker.
	invoker: Invoker,
	p_type: FfiPropertyType,
	id: FfiPropertyId,
	value: FfiPropertyValue,
}

/// A property was modified.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PropertyChanged {
	/// If the name of the invoker is `null`, there is no invoker.
	invoker: Invoker,
	p_type: FfiPropertyType,
	id: FfiPropertyId,
	old: FfiPropertyValue,
	new: FfiPropertyValue,
}
