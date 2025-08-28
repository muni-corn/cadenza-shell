use crate::icon_names::*;

// Icon constants for various widgets
pub const BRIGHTNESS_ICON_NAMES: &[&str] = &[
    DISPLAY_BRIGHTNESS_LOW,
    DISPLAY_BRIGHTNESS_MEDIUM,
    DISPLAY_BRIGHTNESS_HIGH,
];
pub const VOLUME_ICONS: &[&str] = &["󰕿", "󰖀", "󰕾"];
pub const MUTE_ICON: &str = "󰖁";
pub const BATTERY_ICON_NAMES: &[&str] = &[
    BATTERY_LEVEL_30,
    BATTERY_LEVEL_40,
    BATTERY_LEVEL_50,
    BATTERY_LEVEL_60,
    BATTERY_LEVEL_70,
    BATTERY_LEVEL_80,
    BATTERY_LEVEL_90,
    BATTERY_LEVEL_100,
];

pub const NETWORK_WIFI_ICONS: &[&str] = &["󰤯", "󰤟", "󰤢", "󰤥", "󰤨"];
pub const NETWORK_WIRED_ICONS: &[&str] = &["󰈀"];

pub const BLUETOOTH_ICONS: &[&str] = &["󰂯", "󰂱"];

/// Get an icon from a list based on a percentage value
pub fn percentage_to_icon_from_list<'a>(percentage: f64, icons: &'a [&'a str]) -> &'a str {
    let index = (percentage * icons.len() as f64) as usize;
    icons[index.min(icons.len() - 1)]
}
