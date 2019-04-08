use std::ffi::{CStr, CString};
use std::os::raw::c_char;

use futures::Future;

use crate::{Error, Event, EVENTS, FutureHandle, Result, RUNTIME};

/// Allocates a `CString` which has to be freed by the receiver afterwards.
///
/// Returns a null pointer if the string contains a 0 character.
#[inline]
fn str_to_ffi(s: &str) -> *mut c_char {
	let ptr = CString::new(s).map(|s| s.into_raw()).unwrap_or(std::ptr::null_mut());
	//println!("Create {:?}", ptr);
	ptr
}

// Take a reference to get the lifetime of the resulting `str`.
#[inline]
pub(crate) fn ffi_to_str(ptr: &*const c_char) -> Result<&str> {
	let s = unsafe { CStr::from_ptr(*ptr) }.to_str().map_err(Error::from);
	//println!("Read {:?} Len {:?}", ptr, s.as_ref().map(|s| s.len()));
	s
}

pub trait ToFfi {
	type FfiType;
	fn ffi(&self) -> Self::FfiType;
}

impl ToFfi for Error {
	type FfiType = *mut c_char;
	fn ffi(&self) -> Self::FfiType {
		// TODO Use translatable string
		str_to_ffi(&self.to_string())
	}
}

impl<T> ToFfi for Result<T> {
	type FfiType = *mut c_char;
	fn ffi(&self) -> Self::FfiType {
		self.as_ref().err().map(ToFfi::ffi)
			.unwrap_or(std::ptr::null_mut())
	}
}

impl ToFfi for str {
	type FfiType = *mut c_char;
	fn ffi(&self) -> Self::FfiType {
		str_to_ffi(self)
	}
}

pub(crate) trait ToFfiFutureExt {
	fn fut_ffi(self) -> FutureHandle;
}

impl<F: Future<Item=(), Error=Error> + Send + 'static> ToFfiFutureExt for F {
	fn fut_ffi(self) -> FutureHandle {
		let h = FutureHandle::next_free();
		RUNTIME.executor().spawn(self.then(move |r| {
			/*if let Err(e) = &r {
				warn!(LOGGER, "Future exited with error"; "error" => ?e);
			}*/
			EVENTS.0.send(Event::FutureFinished(h, r)).unwrap();
			Ok(())
		}));
		h
	}
}

impl ToFfiFutureExt for Error {
	fn fut_ffi(self) -> FutureHandle {
		let h = FutureHandle::next_free();
		EVENTS.0.send(Event::FutureFinished(h, Err(self))).unwrap();
		h
	}
}
