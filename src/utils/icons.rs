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

pub const NETWORK_WIFI: &str = RADIOWAVES_1;
pub const NETWORK_WIFI_ICON_NAMES: &[&str] =
    &[RADIOWAVES_4, RADIOWAVES_3, RADIOWAVES_2, RADIOWAVES_1];
pub const NETWORK_WIFI_DISABLED: &str = RADIOWAVES_NO;
pub const NETWORK_WIFI_DISCONNECTED: &str = RADIOWAVES_X;
pub const NETWORK_WIRED_DISABLED: &str = RADIOWAVES_NO;
pub const NETWORK_WIRED_CONNECTED: &str = LAN;
pub const NETWORK_WIRED_UNREACHABLE: &str = LAN_QUESTION;

/// Get an icon from a list based on a percentage value from 0.0 to 1.0.
pub fn percentage_to_icon_from_list<'a>(percentage: f64, icons: &'a [&'a str]) -> &'a str {
    let index = ((percentage * icons.len() as f64) as usize).clamp(0, icons.len() - 1);
    icons[index]
}
