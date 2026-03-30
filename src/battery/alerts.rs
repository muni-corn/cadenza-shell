use crate::sound;

// SoC at which a battery's charge is considered low.
pub const BATTERY_THRESHOLD_LOW: f32 = 0.2;

// SoC at which a battery's charge is considered critically low.
pub const BATTERY_THRESHOLD_CRITICAL: f32 = 0.1;

/// XDG sound event id for battery warning (20%).
const SOUND_BATTERY_LOW: &str = "battery-low";

/// XDG sound event id for battery critical (10%).
const SOUND_BATTERY_CRITICAL: &str = "battery-critical";

/// Tracks which low-battery alerts have already fired in the current
/// discharging session, so each is only triggered once.
pub(super) struct AlertState {
    /// Whether the "battery low" (20%) alert has fired this session.
    warn_triggered: bool,

    /// Whether the "battery critical" (10%) alert has fired this session.
    critical_triggered: bool,
}

impl AlertState {
    pub fn new() -> Self {
        Self {
            warn_triggered: false,
            critical_triggered: false,
        }
    }

    /// Reset alert flags at the start of a new discharging session.
    pub fn reset(&mut self) {
        self.warn_triggered = false;
        self.critical_triggered = false;
    }

    /// Fire any pending low-battery alerts based on the current percentage.
    ///
    /// Each alert is sent at most once per discharging session. Critical takes
    /// priority: if we cross 10% without having seen 20% first (e.g. the shell
    /// started with the battery already below 20%), both would fire
    /// independently.
    pub async fn check(&mut self, percentage: f32) {
        // critical alert (10%)
        if percentage <= BATTERY_THRESHOLD_CRITICAL && !self.critical_triggered {
            // automatically set warn_triggered to true here, because it's implied
            self.warn_triggered = true;
            self.critical_triggered = true;
            log::info!("battery critical alert ({:.0}%)", percentage * 100.0);
            fire_battery_alert(AlertLevel::Critical).await;
        } else if percentage <= BATTERY_THRESHOLD_LOW && !self.warn_triggered {
            // warning alert (20%)
            self.warn_triggered = true;
            log::info!("battery low alert ({:.0}%)", percentage * 100.0);
            fire_battery_alert(AlertLevel::Normal).await;
        }
    }
}

#[derive(Clone, Copy)]
enum AlertLevel {
    Normal,
    Critical,
}

/// Send a freedesktop notification and play a sound for a battery alert.
async fn fire_battery_alert(level: AlertLevel) {
    let (summary, body, sound_event) = match level {
        AlertLevel::Critical => (
            "Battery level is critically low",
            "Connect a charger now to avoid losing unsaved work.",
            SOUND_BATTERY_CRITICAL,
        ),
        AlertLevel::Normal => (
            "Battery level is low",
            "Connect a charger to continue using your device.",
            SOUND_BATTERY_LOW,
        ),
    };

    // play sound first so any D-Bus latency doesn't delay the audio cue
    sound::play(sound_event);

    // urgency values per the freedesktop notification spec:
    // 0 = low, 1 = normal, 2 = critical
    let urgency: u8 = match level {
        AlertLevel::Normal => 1,
        AlertLevel::Critical => 2,
    };

    send_notification(summary, body, urgency).await;
}

/// Send a notification via the org.freedesktop.Notifications D-Bus interface.
///
/// This sends to our own daemon, which will display it in the shell's fresh
/// notification overlay and store it in the notification center.
async fn send_notification(summary: &str, body: &str, urgency: u8) {
    let connection = match zbus::Connection::session().await {
        Ok(c) => c,
        Err(e) => {
            log::warn!("couldn't open D-Bus session for battery alert: {}", e);
            return;
        }
    };

    // build the hints dict `a{sv}` with the urgency byte.
    // HashMap<String, OwnedValue> serializes to `a{sv}` because OwnedValue's
    // D-Bus signature is `v` (variant).
    use std::collections::HashMap;

    use zbus::zvariant::{OwnedValue, Value};
    let mut hints: HashMap<String, OwnedValue> = HashMap::new();
    if let Ok(urgency_owned) = OwnedValue::try_from(Value::U8(urgency)) {
        hints.insert("urgency".to_string(), urgency_owned);
    }

    let result = connection
        .call_method(
            Some("org.freedesktop.Notifications"),
            "/org/freedesktop/Notifications",
            Some("org.freedesktop.Notifications"),
            "Notify",
            &(
                "System", // app_name
                0u32,     // replaces_id
                "",       // app_icon
                summary,
                body,
                Vec::<String>::new(), // actions
                hints,
                -1i32, // expire_timeout (-1 = server decides)
            ),
        )
        .await;

    if let Err(e) = result {
        log::warn!("couldn't send battery alert notification: {}", e);
    }
}
