use ::*;
use errors::Error;
use permissions::PermissionId;

pub trait Notification {
    fn get_notification_type() -> NotificationType;
}

pub trait Response {
    fn get_return_code(&self) -> &str;
    fn set_return_code(&mut self, return_code: String);
}

pub trait TryParse<T>: Sized {
    type Err;
    fn try_from(T) -> Result<Self, Self::Err>;
}

include!(concat!(env!("OUT_DIR"), "/structs.rs"));
