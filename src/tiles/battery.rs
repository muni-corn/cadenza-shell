use crate::services::battery::BatteryService;
use crate::utils::icons::{BATTERY_CHARGING_ICONS, BATTERY_ICONS, percentage_to_icon_from_list};
use crate::widgets::tile::{Attention, Tile};
use gtk4::glib;
use gtk4::prelude::*;

pub struct BatteryWidget {
    tile: Tile,
    service: BatteryService,
}

impl BatteryWidget {
    pub fn new() -> Self {
        let tile = Tile::builder()
            .icon("")
            .primary("")
            .visible(false) // Start hidden, will show if available
            .attention(Attention::Dim)
            .build();

        // Add battery-specific CSS class
        tile.add_css_class("battery");

        let service = BatteryService::new();

        // Only show tile if battery is available
        service
            .bind_property("available", &tile, "tile-visible")
            .sync_create()
            .build();

        // Update battery display based on state
        let update_display = glib::clone!(
            #[weak]
            tile,
            #[weak]
            service,
            move || {
                let percentage = service.percentage();
                let is_charging = service.charging();
                let is_low = service.is_low();
                let is_critical = service.is_critical();

                // Choose appropriate icon
                let icon = if is_charging {
                    percentage_to_icon_from_list(percentage, BATTERY_CHARGING_ICONS)
                } else {
                    percentage_to_icon_from_list(percentage, BATTERY_ICONS)
                };
                tile.set_tile_icon(Some(icon.to_string()));

                // Set percentage as primary text
                tile.set_tile_primary(Some(format!("{}%", (percentage * 100.0) as u32)));

                // Set charging status as secondary text
                let status = if is_charging {
                    "Charging"
                } else if is_critical {
                    "Critical"
                } else if is_low {
                    "Low"
                } else {
                    ""
                };

                if !status.is_empty() {
                    tile.set_tile_secondary(Some(status.to_string()));
                } else {
                    tile.set_tile_secondary(None);
                }

                // Update attention state based on battery condition
                let attention = if is_critical {
                    Attention::Alarm
                } else if is_low {
                    Attention::Warning
                } else if is_charging {
                    Attention::Normal
                } else {
                    Attention::Dim
                };
                tile.set_tile_attention(attention);

                // Add battery-specific CSS classes
                tile.remove_css_class("battery-low");
                tile.remove_css_class("battery-critical");
                tile.remove_css_class("battery-charging");

                if is_charging {
                    tile.add_css_class("battery-charging");
                } else if is_critical {
                    tile.add_css_class("battery-critical");
                } else if is_low {
                    tile.add_css_class("battery-low");
                }
            }
        );

        // Connect to property changes
        service.connect_percentage_notify(glib::clone!(
            #[strong]
            update_display,
            move |_| {
                update_display();
            }
        ));

        service.connect_charging_notify(glib::clone!(
            #[strong]
            update_display,
            move |_| {
                update_display();
            }
        ));

        // Initial display update if available
        if service.available() {
            update_display();
        }

        Self { tile, service }
    }

    pub fn widget(&self) -> &gtk4::Widget {
        self.tile.upcast_ref()
    }

    pub fn service(&self) -> &BatteryService {
        &self.service
    }
}
