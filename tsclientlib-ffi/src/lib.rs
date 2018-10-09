extern crate tokio;
extern crate tsclientlib;

use std::ffi::CStr;
use std::os::raw::c_char;
use std::thread;

use tokio::prelude::Future;
use tsclientlib::{Connection, ConnectOptions};

type Result<T> = std::result::Result<T, tsclientlib::Error>;

#[no_mangle]
pub extern "C" fn connect(address: *const c_char) -> Result<()> {
	let address = unsafe { CStr::from_ptr(address) };
	let options = ConnectOptions::new(address.to_str()?);

	thread::spawn(move || tokio::run(
		Connection::new(options)
		.map(|_con| ())
		.map_err(|_| ())
	));

	Ok(())
}
