use crate::services::clock::ClockService;
use crate::widgets::tile::{Attention, Tile};
use gtk4::glib;
use gtk4::prelude::*;

// Clock icons matching the TypeScript version
const CLOCK_ICONS: &[&str] = &[
    "\u{F1456}", // 12 o'clock
    "\u{F144B}", // 1 o'clock
    "\u{F144C}", // 2 o'clock
    "\u{F144D}", // 3 o'clock
    "\u{F144E}", // 4 o'clock
    "\u{F144F}", // 5 o'clock
    "\u{F1450}", // 6 o'clock
    "\u{F1451}", // 7 o'clock
    "\u{F1452}", // 8 o'clock
    "\u{F1453}", // 9 o'clock
    "\u{F1454}", // 10 o'clock
    "\u{F1455}", // 11 o'clock
];

pub struct ClockWidget {
    tile: Tile,
    service: ClockService,
}

impl ClockWidget {
    pub fn new() -> Self {
        let tile = Tile::builder()
            .icon("")
            .primary("")
            .secondary("")
            .visible(true)
            .attention(Attention::Normal)
            .build();

        // Add clock-specific CSS class
        tile.add_css_class("clock");

        let service = ClockService::new();

        // Bind time string to primary text
        service
            .bind_property("time-string", &tile, "primary")
            .sync_create()
            .build();

        // Bind date string to secondary text
        service
            .bind_property("date-string", &tile, "secondary")
            .sync_create()
            .build();

        // Update icon based on hour (12-hour format)
        let update_icon = glib::clone!(
            #[weak]
            tile,
            #[weak]
            service,
            move || {
                let hour = service.hour();
                let hour_12 = (hour % 12) as usize;
                let icon = CLOCK_ICONS.get(hour_12).unwrap_or(&CLOCK_ICONS[0]);
                tile.set_tile_icon(Some(icon.to_string()));
            }
        );

        // Connect to hour changes to update icon
        service.connect_hour_notify(glib::clone!(
            #[strong]
            update_icon,
            move |_| {
                update_icon();
            }
        ));

        // Initial icon update
        update_icon();

        Self { tile, service }
    }

    pub fn widget(&self) -> &gtk4::Widget {
        self.tile.upcast_ref()
    }

    pub fn service(&self) -> &ClockService {
        &self.service
    }
}
