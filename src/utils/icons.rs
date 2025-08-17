// Icon constants for various widgets
pub const BRIGHTNESS_ICONS: &[&str] = &["󰃞", "󰃟", "󰃠", "󰃡", "󰃢", "󰃣"];
pub const VOLUME_ICONS: &[&str] = &["󰕿", "󰖀", "󰕾"];
pub const MUTE_ICON: &str = "󰖁";
pub const BATTERY_ICONS: &[&str] = &["󰁺", "󰁻", "󰁼", "󰁽", "󰁾", "󰁿", "󰂀", "󰂁", "󰂂", "󰁹"];
pub const BATTERY_CHARGING_ICONS: &[&str] = &["󰢟", "󰢜", "󰂆", "󰂇", "󰂈", "󰢝", "󰂉", "󰢞", "󰂊", "󰂋"];
pub const NETWORK_WIFI_ICONS: &[&str] = &["󰤯", "󰤟", "󰤢", "󰤥", "󰤨"];
pub const NETWORK_WIRED_ICONS: &[&str] = &["󰈀"];
pub const BLUETOOTH_ICONS: &[&str] = &["󰂯", "󰂱"];

/// Get an icon from a list based on a percentage value
pub fn percentage_to_icon_from_list<'a>(percentage: f64, icons: &'a [&'a str]) -> &'a str {
    let index = (percentage * icons.len() as f64) as usize;
    icons[index.min(icons.len() - 1)]
}