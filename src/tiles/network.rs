use crate::services::network::{DeviceType, NetworkService};
use crate::utils::icons::{NETWORK_WIFI_ICONS, NETWORK_WIRED_ICONS, percentage_to_icon_from_list};
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{Box, Label, Orientation};

pub struct NetworkWidget {
    container: Box,
    service: NetworkService,
}

impl NetworkWidget {
    pub fn new() -> Self {
        let container = Box::builder()
            .orientation(Orientation::Horizontal)
            .spacing(8)
            .css_classes(vec!["tile"])
            .build();

        let service = NetworkService::new();

        // Only show widget if network service is available
        service
            .bind_property("available", &container, "visible")
            .sync_create()
            .build();

        if service.available() {
            let icon_label = Label::builder()
                .css_classes(vec!["icon", "dim"])
                .width_request(16)
                .build();

            // Update icon and styling based on network state
            let update_display = glib::clone!(
                #[weak]
                icon_label,
                #[weak]
                service,
                move || {
                    let is_connected = service.connected();
                    let device_type = service.primary_device_type();

                    // Choose appropriate icon
                    let icon = if is_connected {
                        match device_type {
                            DeviceType::Wifi => {
                                let strength = service.wifi_strength() as f64 / 100.0;
                                percentage_to_icon_from_list(strength, NETWORK_WIFI_ICONS)
                            }
                            DeviceType::Ethernet => NETWORK_WIRED_ICONS[0],
                            _ => NETWORK_WIFI_ICONS[0], // Default to lowest WiFi icon
                        }
                    } else {
                        NETWORK_WIFI_ICONS[0] // Disconnected icon
                    };

                    icon_label.set_text(icon);

                    // Update CSS classes based on connection state
                    icon_label.remove_css_class("network-connected");
                    icon_label.remove_css_class("network-disconnected");
                    icon_label.remove_css_class("network-wifi");
                    icon_label.remove_css_class("network-ethernet");

                    if is_connected {
                        icon_label.add_css_class("network-connected");
                        match device_type {
                            DeviceType::Wifi => icon_label.add_css_class("network-wifi"),
                            DeviceType::Ethernet => icon_label.add_css_class("network-ethernet"),
                            _ => {}
                        }
                    } else {
                        icon_label.add_css_class("network-disconnected");
                    }

                    // Trigger fade animation when state changes
                    icon_label.remove_css_class("dim");
                    icon_label.add_css_class("bright");

                    glib::timeout_add_local_once(
                        std::time::Duration::from_secs(3),
                        glib::clone!(
                            #[weak]
                            icon_label,
                            move || {
                                icon_label.remove_css_class("bright");
                                icon_label.add_css_class("dim");
                            }
                        ),
                    );
                }
            );

            // Connect to property changes
            service.connect_connected_notify(glib::clone!(
                #[strong]
                update_display,
                move |_| {
                    update_display();
                }
            ));

            service.connect_wifi_enabled_notify(glib::clone!(
                #[strong]
                update_display,
                move |_| {
                    update_display();
                }
            ));

            service.connect_ethernet_connected_notify(glib::clone!(
                #[strong]
                update_display,
                move |_| {
                    update_display();
                }
            ));

            service.connect_wifi_strength_notify(glib::clone!(
                #[strong]
                update_display,
                move |_| {
                    update_display();
                }
            ));

            // Initial display update
            update_display();

            container.append(&icon_label);
        }

        Self { container, service }
    }

    pub fn widget(&self) -> &Box {
        &self.container
    }

    pub fn service(&self) -> &NetworkService {
        &self.service
    }
}
