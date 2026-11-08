use serde_repr::{Deserialize_repr, Serialize_repr};
use zbus::zvariant::Type;

#[derive(Debug, Clone)]
pub struct Notification {
    pub id: u32,
    pub app_name: String,
    pub app_icon: String,
    pub desktop_entry: String,
    pub image: String,
    pub summary: String,
    pub body: String,
    pub urgency: NotificationUrgency,
    // part of the freedesktop spec; retained for completeness even if not
    // currently used
    #[allow(dead_code)]
    pub timeout: i32,
    pub timestamp: i64,

    /// Action identifiers paired with their user-facing labels.
    pub actions: Vec<(String, String)>,
}

#[derive(Serialize_repr, Deserialize_repr, PartialEq, Default, Debug, Type, Clone, Copy)]
#[repr(u8)]
pub enum NotificationUrgency {
    Low = 0,

    #[default]
    Normal = 1,

    Critical = 2,
}
