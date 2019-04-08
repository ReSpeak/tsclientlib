use std::default::Default;
use std::os::raw::c_char;
use std::ptr;

use tsclientlib::{MaxClients, TalkPowerRequest};

use crate::ffi_utils::ToFfi;

#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct FfiMaxClients {
	limit: u16,
	kind: FfiMaxClientsKind,
}

#[derive(Clone, Copy, Debug)]
#[repr(u8)]
pub enum FfiMaxClientsKind {
	Unlimited,
	Inherited,
	Limited,
}

#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct FfiTalkPowerRequest {
	time: u64,
	message: *mut c_char,
}

impl Default for FfiMaxClients {
	fn default() -> Self {
		Self { limit: 0, kind: FfiMaxClientsKind::Unlimited }
	}
}

impl Default for FfiTalkPowerRequest {
	fn default() -> Self {
		Self { time: 0, message: ptr::null_mut() }
	}
}

impl ToFfi for MaxClients {
	type FfiType = FfiMaxClients;
	fn ffi(&self) -> Self::FfiType {
		FfiMaxClients {
			limit: if let MaxClients::Limited(i) = self { *i } else { 0 },
			kind: match self {
				MaxClients::Unlimited => FfiMaxClientsKind::Unlimited,
				MaxClients::Inherited => FfiMaxClientsKind::Inherited,
				MaxClients::Limited(_) => FfiMaxClientsKind::Limited,
			}
		}
	}
}

impl ToFfi for TalkPowerRequest {
	type FfiType = FfiTalkPowerRequest;
	fn ffi(&self) -> Self::FfiType {
		FfiTalkPowerRequest {
			// TODO Higher resolution?
			time: self.time.timestamp() as u64,
			message: self.message.ffi(),
		}
	}
}
