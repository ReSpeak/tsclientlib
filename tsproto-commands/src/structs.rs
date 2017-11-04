use ::*;
use errors::Error;
use permissions::PermissionId;

trait Notification {
	fn get_notification_type() -> NotificationType;
}

trait Response {
	fn get_return_code(&self) -> &str;
	fn set_return_code(&mut self, return_code: String);
}

include!(concat!(env!("OUT_DIR"), "/structs.rs"));
