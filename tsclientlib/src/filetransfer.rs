use std::net::IpAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU16, Ordering};

use chashmap::CHashMap;
use failure::format_err;
use futures::sync::mpsc;
use ts_bookkeeping::messages::s2c::{InFileDownload, InFileTransferStatus,
	InFileUpload, InMessageTrait};
use tsproto_packets::packets::InCommand;

use crate::TsError;

pub(crate) enum FileTransferStatus {
	Start {
		key: String,
		port: u16,
		size: u64,
		ip: Option<IpAddr>,
	},
	Status {
		status: TsError,
	},
}

pub(crate) struct FileTransferIdHandle {
	handler: Arc<FileTransferHandler>,
	pub id: u16,
}

pub(crate) struct FileTransferHandler {
	ids: CHashMap<u16, mpsc::UnboundedSender<FileTransferStatus>>,
	cur_id: AtomicU16,
}

impl Drop for FileTransferIdHandle {
	fn drop(&mut self) {
		self.handler.ids.remove(&self.id);
	}
}

impl FileTransferHandler {
	pub(crate) fn new() -> Self {
		Self {
			ids: CHashMap::new(),
			cur_id: AtomicU16::new(0),
		}
	}

	/// Get a return code and a receiver which gets notified when an answer is
	/// received.
	pub(crate) fn get_file_transfer_id(
		handler: Arc<Self>,
	) -> (FileTransferIdHandle, mpsc::UnboundedReceiver<FileTransferStatus>) {
		let code = handler.cur_id.fetch_add(1, Ordering::Relaxed);
		let (send, recv) = mpsc::unbounded();
		// The receiver should fail when the sender is dropped, but usize should
		// be enough for every platform.
		handler.ids.insert(code, send);
		let code = FileTransferIdHandle {
			handler,
			id: code,
		};
		(code, recv)
	}

	/// Return `true` if the file transfer id was found and the packet was
	/// handled, `false` if not.
	pub(crate) fn handle_start_download(&self, cmd: &InCommand) -> Result<bool, tsproto::Error> {
		let msg = InFileDownload::new(cmd)
			.map_err(|e| format_err!("Cannot parse notifystartdownload \
				({:?})", e))?;
		let msg = msg.iter().next().unwrap();

		if let Some(ft) = self.ids.get_mut(&msg.client_file_transfer_id) {
			let status = FileTransferStatus::Start {
				key: msg.file_transfer_key.into(),
				port: msg.port,
				size: msg.size,
				ip: msg.ip,
			};
			// Ignore if sending fails
			let _ = ft.unbounded_send(status);
			Ok(true)
		} else {
			Ok(false)
		}
	}

	/// Return `true` if the file transfer id was found and the packet was
	/// handled, `false` if not.
	pub(crate) fn handle_start_upload(&self, cmd: &InCommand) -> Result<bool, tsproto::Error> {
		let msg = InFileUpload::new(cmd)
			.map_err(|e| format_err!("Cannot parse notifystartdownload \
				({:?})", e))?;
		let msg = msg.iter().next().unwrap();

		if let Some(ft) = self.ids.get_mut(&msg.client_file_transfer_id) {
			let status = FileTransferStatus::Start {
				key: msg.file_transfer_key.into(),
				port: msg.port,
				size: msg.seek_position,
				ip: msg.ip,
			};
			// Ignore if sending fails
			let _ = ft.unbounded_send(status);
			Ok(true)
		} else {
			Ok(false)
		}
	}

	/// Return `true` if the file transfer id was found and the packet was
	/// handled, `false` if not.
	pub(crate) fn handle_status(&self, cmd: &InCommand) -> Result<bool, tsproto::Error> {
		let msg = InFileTransferStatus::new(cmd)
			.map_err(|e| format_err!("Cannot parse notifystatusfiletransfer \
				({:?})", e))?;
		let msg = msg.iter().next().unwrap();

		if let Some(ft) = self.ids.get_mut(&msg.client_file_transfer_id) {
			let status = FileTransferStatus::Status {
				status: msg.status,
			};
			// Ignore if sending fails
			let _ = ft.unbounded_send(status);
			Ok(true)
		} else {
			Ok(false)
		}
	}
}
