use crate::icon_names::*;

// icon constants for various widgets
pub const BRIGHTNESS_ICON_NAMES: &[&str] = &[BRIGHTNESS_LOW_REGULAR, BRIGHTNESS_HIGH_REGULAR];

pub const VOLUME_ICONS: &[&str] = &[SPEAKER_0_REGULAR, SPEAKER_1_REGULAR, SPEAKER_2_REGULAR];
pub const VOLUME_MUTED: &str = SPEAKER_OFF_REGULAR;
pub const VOLUME_ZERO: &str = SPEAKER_MUTE_REGULAR;

pub const BATTERY_ICON_NAMES: &[&str] = &[
    BATTERY_1_REGULAR,
    BATTERY_2_REGULAR,
    BATTERY_3_REGULAR,
    BATTERY_4_REGULAR,
    BATTERY_5_REGULAR,
    BATTERY_6_REGULAR,
    BATTERY_7_REGULAR,
    BATTERY_8_REGULAR,
    BATTERY_9_REGULAR,
    BATTERY_10_REGULAR,
];

pub const NETWORK_WIFI_ICON_NAMES: &[&str] = &[
    WIFI_4_REGULAR,
    WIFI_3_REGULAR,
    WIFI_2_REGULAR,
    WIFI_1_REGULAR,
];

/// Get an icon from a list based on a percentage value from 0.0 to 1.0.
pub fn percentage_to_icon_from_list<'a>(percentage: f64, icons: &'a [&'a str]) -> &'a str {
    let index = ((percentage * icons.len() as f64) as usize).clamp(0, icons.len() - 1);
    icons[index]
}
