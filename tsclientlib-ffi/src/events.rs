//! Events for property changes

use std::os::raw::{c_char, c_void};

use crate::{FfiMaxClients, FfiTalkPowerRequest};

include!(concat!(env!("OUT_DIR"), "/events.rs"));

#[repr(C)]
#[derive(Clone, Copy)]
pub struct FfiProperty {
	p_type: FfiPropertyType,
	id: FfiPropertyId,
	value: FfiPropertyValue,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct PropertyChanged {
	p_type: FfiPropertyType,
	id: FfiPropertyId,
	old: FfiPropertyValue,
	new: FfiPropertyValue,
}
