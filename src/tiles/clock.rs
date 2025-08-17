use crate::services::clock::ClockService;
use gtk4::prelude::*;
use gtk4::{Box, Label, Orientation};
use gtk4::glib;

pub struct ClockWidget {
    container: Box,
    service: ClockService,
}

impl ClockWidget {
    pub fn new() -> Self {
        let container = Box::builder()
            .orientation(Orientation::Vertical)
            .spacing(2)
            .css_classes(vec!["tile", "clock"])
            .halign(gtk4::Align::Center)
            .build();

        let service = ClockService::new();

        let time_label = Label::builder()
            .css_classes(vec!["time"])
            .halign(gtk4::Align::Center)
            .build();

        let date_label = Label::builder()
            .css_classes(vec!["date"])
            .halign(gtk4::Align::Center)
            .build();

        // Bind time and date strings to labels
        service
            .bind_property("time-string", &time_label, "label")
            .sync_create()
            .build();

        service
            .bind_property("date-string", &date_label, "label")
            .sync_create()
            .build();

        container.append(&time_label);
        container.append(&date_label);

        Self { container, service }
    }

    pub fn widget(&self) -> &Box {
        &self.container
    }

    pub fn service(&self) -> &ClockService {
        &self.service
    }
}