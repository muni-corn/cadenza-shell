use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use zbus::zvariant::Type;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    pub id: u32,
    pub app_name: String,
    pub app_icon: String,
    pub desktop_entry: String,
    pub image: String,
    pub summary: String,
    pub body: String,
    pub urgency: NotificationUrgency,
    pub timeout: i32,
    pub timestamp: i64,
    pub actions: Vec<String>,
}

#[derive(Serialize_repr, Deserialize_repr, PartialEq, Default, Debug, Type, Clone, Copy)]
#[repr(u8)]
pub enum NotificationUrgency {
    Low = 0,

    #[default]
    Normal = 1,

    Critical = 2,
}
