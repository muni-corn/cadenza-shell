use serde::{Deserialize, Deserializer};
use serde_repr::{Deserialize_repr, Serialize_repr};
use zbus::zvariant::Type;

fn de_actions<'de, D>(deserializer: D) -> Result<Vec<(String, String)>, D::Error>
where
    D: Deserializer<'de>,
{
    // actions are represented as a list of strings. even-indexed strings are
    // identifiers; odd-indexed strings are the user-facing labels presented on
    // action buttons
    let v = Vec::<String>::deserialize(deserializer)?;
    let evens = v.iter().step_by(2).cloned();
    let odds = v.iter().skip(1).step_by(2).cloned();
    Ok(evens.zip(odds).collect())
}

#[derive(Debug, Clone, Deserialize)]
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

    #[serde(deserialize_with = "de_actions")]
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
