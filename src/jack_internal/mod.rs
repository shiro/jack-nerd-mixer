pub mod notifications;

pub(crate) struct IgnoreNotifications;

impl jack::NotificationHandler for IgnoreNotifications {}
