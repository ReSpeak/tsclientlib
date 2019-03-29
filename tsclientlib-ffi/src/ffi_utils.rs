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

pub trait ToFfiStringExt {
	fn ffi(&self) -> *mut c_char;
}

impl ToFfiStringExt for Error {
	fn ffi(&self) -> *mut c_char {
		// TODO Use translatable string
		str_to_ffi(&self.to_string())
	}
}

impl<T> ToFfiStringExt for Result<T> {
	fn ffi(&self) -> *mut c_char {
		self.as_ref().err().map(ToFfiStringExt::ffi)
			.unwrap_or(std::ptr::null_mut())
	}
}

impl ToFfiStringExt for str {
	fn ffi(&self) -> *mut c_char {
		str_to_ffi(self)
	}
}

pub(crate) trait ToFfiFutureExt {
	fn ffi(self) -> FutureHandle;
}

impl<F: Future<Item=(), Error=Error> + Send + 'static> ToFfiFutureExt for F {
	fn ffi(self) -> FutureHandle {
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
