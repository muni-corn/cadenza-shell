use crate::icon_names::*;

// Icon constants for various widgets
pub const BRIGHTNESS_ICON_NAMES: &[&str] = &[
    DISPLAY_BRIGHTNESS_LOW,
    DISPLAY_BRIGHTNESS_MEDIUM,
    DISPLAY_BRIGHTNESS_HIGH,
];
pub const VOLUME_ICONS: &[&str] = &[SPEAKER_1, SPEAKER_2, SPEAKER_3];
pub const MUTE_ICON: &str = SPEAKER_0;
pub const BATTERY_ICON_NAMES: &[&str] = &[
    BATTERY_LEVEL_30,
    BATTERY_LEVEL_30,
    BATTERY_LEVEL_30,
    BATTERY_LEVEL_30,
    BATTERY_LEVEL_40,
    BATTERY_LEVEL_50,
    BATTERY_LEVEL_60,
    BATTERY_LEVEL_70,
    BATTERY_LEVEL_80,
    BATTERY_LEVEL_90,
    BATTERY_LEVEL_100,
];

pub const NETWORK_WIFI_ICON_NAMES: &[&str] =
    &[RADIOWAVES_4, RADIOWAVES_3, RADIOWAVES_2, RADIOWAVES_1];
pub const NETWORK_WIRED_ICONS: &[&str] = &["󰈀"];

pub const BLUETOOTH_ICONS: &[&str] = &["󰂯", "󰂱"];

/// Get an icon from a list based on a percentage value from 0.0 to 1.0.
pub fn percentage_to_icon_from_list<'a>(percentage: f64, icons: &'a [&'a str]) -> &'a str {
    let index = if percentage <= 0.0 {
        0
    } else if percentage >= 1.0 {
        icons.len() - 1
    } else {
        ((percentage * (icons.len() - 1) as f64).round() as usize).min(icons.len() - 1)
    };
    icons[index]
}
