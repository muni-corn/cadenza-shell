use crate::icon_names::*;

// icon constants for various widgets
pub const BRIGHTNESS_ICON_NAMES: &[&str] = &[DISPLAY_BRIGHTNESS];

pub const VOLUME_ICONS: &[&str] = &[SPEAKER_MIN, SPEAKER_MID, SPEAKER_MAX];
pub const VOLUME_MUTED: &str = SPEAKER_CROSS;
pub const VOLUME_ZERO: &str = SPEAKER_CROSS;

pub const BATTERY_ICON_NAMES: &[&str] = &[
    BATTERY_EMPTY,
    BATTERY_10,
    BATTERY_20,
    BATTERY_30,
    BATTERY_40,
    BATTERY_50,
    BATTERY_60,
    BATTERY_70,
    BATTERY_80,
    BATTERY_90,
    BATTERY_100,
];

pub const BATTERY_CHARGING_ICON_NAMES: &[&str] = &[
    BATTERY_0_CH,
    BATTERY_10_CH,
    BATTERY_20_CH,
    BATTERY_30_CH,
    BATTERY_40_CH,
    BATTERY_50_CH,
    BATTERY_60_CH,
    BATTERY_70_CH,
    BATTERY_80_CH,
    BATTERY_90_CH,
    BATTERY_100_CH,
];

// the radiowaves-N icons are named strongest-to-weakest (radiowaves-1 is
// full signal, radiowaves-4 is empty), not low-to-high like the battery and
// volume lists below, so this order must stay descending to match
// percentage_to_icon_from_list's low-to-high indexing
pub const NETWORK_WIFI_ICON_NAMES: &[&str] =
    &[RADIOWAVES_4, RADIOWAVES_3, RADIOWAVES_2, RADIOWAVES_1];
/// The wifi radio itself is switched off.
pub const NETWORK_WIFI_DISABLED: &str = RADIOWAVES_NO;
/// The wifi radio is on, but nothing is connected.
pub const NETWORK_WIFI_DISCONNECTED: &str = RADIOWAVES_X;
/// Connected to wifi, but the connectivity check found no (or only a
/// limited/captive-portal) route to the internet.
pub const NETWORK_WIFI_NO_ROUTE: &str = RADIOWAVES_QUESTION;
pub const NETWORK_WIRED_CONNECTED: &str = LAN;
/// Connected over ethernet, but the connectivity check found no (or only a
/// limited/captive-portal) route to the internet.
pub const NETWORK_WIRED_NO_ROUTE: &str = LAN_QUESTION;
/// A wifi connection (or one of unknown kind) is being established.
pub const NETWORK_WIFI_CONNECTING: &str = RADIOWAVES_DOTS;
/// An ethernet connection is being established.
pub const NETWORK_WIRED_CONNECTING: &str = LAN_DOTS;

/// Get an icon from a list based on a percentage value from 0.0 to 1.0.
pub fn percentage_to_icon_from_list<'a>(percentage: f64, icons: &'a [&'a str]) -> &'a str {
    let index = ((percentage * icons.len() as f64) as usize).clamp(0, icons.len() - 1);
    icons[index]
}
