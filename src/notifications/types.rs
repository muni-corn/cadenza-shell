use std::time::Duration;

use crate::notifications::image::NotificationImage;

#[derive(Debug, Clone)]
pub struct Notification {
    pub id: u32,
    pub app_name: String,
    pub app_icon: String,
    pub desktop_entry: String,
    /// The image from the `image-data` or `image-path` hint, if any.
    pub image: Option<NotificationImage>,
    pub summary: String,
    pub body: String,
    pub urgency: NotificationUrgency,
    /// The client's `expire_timeout` in milliseconds: -1 leaves it to the
    /// server and 0 means never.
    pub timeout: i32,
    pub timestamp: i64,

    /// Action identifiers paired with their user-facing labels.
    pub actions: Vec<(String, String)>,
    /// Whether the notification stays open after an action is invoked (the
    /// `resident` hint).
    pub resident: bool,
    /// Whether action ids are icon names to show instead of their labels
    /// (the `action-icons` hint).
    pub action_icons: bool,
}

/// The action invoked by clicking the notification itself.
pub const DEFAULT_ACTION: &str = "default";

impl Notification {
    /// How long this notification's popup stays on screen, or `None` to keep
    /// it until the notification is closed.
    ///
    /// Critical notifications always stay. Otherwise the client's timeout
    /// wins, and -1 falls back to `default_secs` (where 0 also means stay).
    pub fn popup_duration(&self, default_secs: u64) -> Option<Duration> {
        if self.urgency == NotificationUrgency::Critical {
            return None;
        }

        match self.timeout {
            0 => None,
            timeout if timeout > 0 => Some(Duration::from_millis(timeout as u64)),
            _ => (default_secs > 0).then(|| Duration::from_secs(default_secs)),
        }
    }
}

#[derive(PartialEq, Eq, Default, Debug, Clone, Copy)]
pub enum NotificationUrgency {
    Low,
    #[default]
    Normal,
    Critical,
}

impl From<u8> for NotificationUrgency {
    /// Maps the spec's urgency levels, treating anything unknown as normal.
    fn from(level: u8) -> Self {
        match level {
            0 => Self::Low,
            2 => Self::Critical,
            _ => Self::Normal,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notification(timeout: i32, urgency: NotificationUrgency) -> Notification {
        Notification {
            id: 1,
            app_name: String::new(),
            app_icon: String::new(),
            desktop_entry: String::new(),
            image: None,
            summary: String::new(),
            body: String::new(),
            urgency,
            timeout,
            timestamp: 0,
            actions: Vec::new(),
            resident: false,
            action_icons: false,
        }
    }

    #[test]
    fn client_timeouts_are_in_milliseconds() {
        let n = notification(2500, NotificationUrgency::Normal);
        assert_eq!(n.popup_duration(10), Some(Duration::from_millis(2500)));
    }

    #[test]
    fn server_default_applies_to_minus_one() {
        let n = notification(-1, NotificationUrgency::Normal);
        assert_eq!(n.popup_duration(10), Some(Duration::from_secs(10)));
        assert_eq!(n.popup_duration(0), None);
    }

    #[test]
    fn zero_and_critical_stay_on_screen() {
        assert_eq!(
            notification(0, NotificationUrgency::Low).popup_duration(10),
            None
        );
        assert_eq!(
            notification(5000, NotificationUrgency::Critical).popup_duration(10),
            None
        );
    }
}
