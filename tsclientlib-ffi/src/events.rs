//! Events for property changes
use std::os::raw::{c_char, c_void};

use num::ToPrimitive;
use tsclientlib::events::{Property, PropertyId};

use crate::{FfiMaxClients, FfiTalkPowerRequest, Invoker};
use crate::ffi_utils::ToFfi;

include!(concat!(env!("OUT_DIR"), "/events.rs"));

/// A property was added or removed.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct FfiProperty {
	value: FfiPropertyValue,
	/// If the name of the invoker is `null`, there is no invoker.
	invoker: Invoker,
	p_type: FfiPropertyType,
	id: FfiPropertyId,
	/// When the value is an optional and it is `None`, this boolean is `false`.
	value_exists: bool,
}

/// A property was modified.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct PropertyChanged {
	old: FfiPropertyValue,
	new: FfiPropertyValue,
	/// If the name of the invoker is `null`, there is no invoker.
	invoker: Invoker,
	p_type: FfiPropertyType,
	id: FfiPropertyId,
	/// When the value is an optional and it is `None`, this boolean is `false`.
	old_exists: bool,
	/// When the value is an optional and it is `None`, this boolean is `false`.
	new_exists: bool,
}
