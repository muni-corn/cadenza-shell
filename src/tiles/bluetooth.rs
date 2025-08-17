use crate::utils::icons::BLUETOOTH_ICONS;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{Box, Label, Orientation};
use std::process::Command;

pub struct BluetoothWidget {
    container: Box,
    icon_label: Label,
}

impl BluetoothWidget {
    pub fn new() -> Self {
        let container = Box::builder()
            .orientation(Orientation::Horizontal)
            .spacing(8)
            .css_classes(vec!["tile"])
            .build();

        let icon_label = Label::builder()
            .css_classes(vec!["icon", "dim"])
            .width_request(16)
            .build();

        // Check initial Bluetooth state
        let is_enabled = Self::check_bluetooth_status();
        let icon = if is_enabled {
            BLUETOOTH_ICONS[1] // Connected/enabled
        } else {
            BLUETOOTH_ICONS[0] // Disabled
        };
        icon_label.set_text(icon);

        // Update CSS classes based on state
        if is_enabled {
            icon_label.add_css_class("bluetooth-enabled");
        } else {
            icon_label.add_css_class("bluetooth-disabled");
        }

        // Monitor Bluetooth status every 10 seconds
        let icon_label_clone = icon_label.clone();
        glib::timeout_add_local(std::time::Duration::from_secs(10), move || {
            let is_enabled = Self::check_bluetooth_status();
            let icon = if is_enabled {
                BLUETOOTH_ICONS[1]
            } else {
                BLUETOOTH_ICONS[0]
            };
            icon_label_clone.set_text(icon);

            // Update CSS classes
            icon_label_clone.remove_css_class("bluetooth-enabled");
            icon_label_clone.remove_css_class("bluetooth-disabled");

            if is_enabled {
                icon_label_clone.add_css_class("bluetooth-enabled");
            } else {
                icon_label_clone.add_css_class("bluetooth-disabled");
            }

            // Trigger fade animation
            icon_label_clone.remove_css_class("dim");
            icon_label_clone.add_css_class("bright");

            glib::timeout_add_local_once(
                std::time::Duration::from_secs(3),
                glib::clone!(@weak icon_label_clone => move || {
                    icon_label_clone.remove_css_class("bright");
                    icon_label_clone.add_css_class("dim");
                }),
            );

            glib::ControlFlow::Continue
        });

        container.append(&icon_label);

        Self {
            container,
            icon_label,
        }
    }

    fn check_bluetooth_status() -> bool {
        // Try to check if Bluetooth is powered on using bluetoothctl
        if let Ok(output) = Command::new("bluetoothctl").args(["show"]).output() {
            if output.status.success() {
                let output_str = String::from_utf8_lossy(&output.stdout);
                return output_str.contains("Powered: yes");
            }
        }

        // Fallback: check if bluetooth service is running
        if let Ok(output) = Command::new("systemctl")
            .args(["is-active", "bluetooth"])
            .output()
        {
            if output.status.success() {
                let output_str = String::from_utf8_lossy(&output.stdout);
                return output_str.trim() == "active";
            }
        }

        false
    }

    pub fn widget(&self) -> &Box {
        &self.container
    }
}
