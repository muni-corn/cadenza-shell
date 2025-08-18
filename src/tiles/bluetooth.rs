use crate::utils::icons::BLUETOOTH_ICONS;
use crate::widgets::tile::{Attention, Tile};
use gtk4::glib;
use gtk4::prelude::*;
use std::process::Command;

pub struct BluetoothWidget {
    tile: Tile,
}

impl BluetoothWidget {
    pub fn new() -> Self {
        let tile = Tile::builder()
            .icon("")
            .visible(true)
            .attention(Attention::Dim)
            .build();

        // Add bluetooth-specific CSS class
        tile.add_css_class("bluetooth");

        // Check initial Bluetooth state and update tile
        Self::update_bluetooth_status(&tile);

        // Monitor Bluetooth status every 10 seconds
        let tile_clone = tile.clone();
        glib::timeout_add_local(std::time::Duration::from_secs(10), move || {
            Self::update_bluetooth_status(&tile_clone);
            glib::ControlFlow::Continue
        });

        Self { tile }
    }

    fn update_bluetooth_status(tile: &Tile) {
        let (is_enabled, connected_devices) = Self::check_bluetooth_status();

        // Choose appropriate icon
        let icon = if is_enabled && !connected_devices.is_empty() {
            BLUETOOTH_ICONS[1] // Connected
        } else if is_enabled {
            BLUETOOTH_ICONS[0] // Enabled but not connected
        } else {
            BLUETOOTH_ICONS[0] // Disabled
        };

        tile.set_tile_icon(Some(icon.to_string()));

        // Set status as primary text
        let status = if !connected_devices.is_empty() {
            format!(
                "{} device{}",
                connected_devices.len(),
                if connected_devices.len() == 1 {
                    ""
                } else {
                    "s"
                }
            )
        } else if is_enabled {
            "On".to_string()
        } else {
            "Off".to_string()
        };
        tile.set_tile_primary(Some(status));

        // Set first connected device name as secondary text
        if !connected_devices.is_empty() {
            tile.set_tile_secondary(Some(connected_devices[0].clone()));
        } else {
            tile.set_tile_secondary(None);
        }

        // Update attention state based on bluetooth status
        let attention = if !connected_devices.is_empty() {
            Attention::Normal // Connected devices
        } else if is_enabled {
            Attention::Dim // Enabled but no connections
        } else {
            Attention::Dim // Disabled
        };
        tile.set_tile_attention(attention);

        // Update CSS classes
        tile.remove_css_class("bluetooth-enabled");
        tile.remove_css_class("bluetooth-disabled");
        tile.remove_css_class("bluetooth-connected");

        if !connected_devices.is_empty() {
            tile.add_css_class("bluetooth-connected");
        } else if is_enabled {
            tile.add_css_class("bluetooth-enabled");
        } else {
            tile.add_css_class("bluetooth-disabled");
        }
    }

    fn check_bluetooth_status() -> (bool, Vec<String>) {
        let mut is_enabled = false;
        let mut connected_devices = Vec::new();

        // Try to check Bluetooth status using bluetoothctl
        if let Ok(output) = Command::new("bluetoothctl").args(["show"]).output() {
            if output.status.success() {
                let output_str = String::from_utf8_lossy(&output.stdout);
                is_enabled = output_str.contains("Powered: yes");
            }
        }

        // If enabled, check for connected devices
        if is_enabled {
            if let Ok(output) = Command::new("bluetoothctl")
                .args(["devices", "Connected"])
                .output()
            {
                if output.status.success() {
                    let output_str = String::from_utf8_lossy(&output.stdout);
                    for line in output_str.lines() {
                        if line.starts_with("Device ") {
                            // Extract device name (everything after MAC address)
                            if let Some(name_start) =
                                line.find(' ').and_then(|pos| line[pos + 1..].find(' '))
                            {
                                let device_name =
                                    line[name_start + line.find(' ').unwrap() + 2..].trim();
                                if !device_name.is_empty() {
                                    connected_devices.push(device_name.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }

        // Fallback: check if bluetooth service is running if other methods failed
        if !is_enabled {
            if let Ok(output) = Command::new("systemctl")
                .args(["is-active", "bluetooth"])
                .output()
            {
                if output.status.success() {
                    let output_str = String::from_utf8_lossy(&output.stdout);
                    is_enabled = output_str.trim() == "active";
                }
            }
        }

        (is_enabled, connected_devices)
    }

    pub fn widget(&self) -> &gtk4::Widget {
        self.tile.upcast_ref()
    }
}
