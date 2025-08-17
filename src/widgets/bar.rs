use gtk4::prelude::*;
use gtk4::{Box, Orientation, Label};
use gdk4::Monitor;
use crate::tiles::brightness::BrightnessWidget;
use crate::tiles::volume::VolumeWidget;
use crate::tiles::battery::BatteryWidget;

pub struct Bar {
    container: Box,
    _monitor: Monitor,
}

impl Bar {
    pub fn new(monitor: &Monitor) -> Self {
        let container = Box::builder()
            .orientation(Orientation::Horizontal)
            .css_classes(vec!["bar"])
            .height_request(32)
            .build();

        // Left section - placeholder
        let left_section = Box::builder()
            .orientation(Orientation::Horizontal)
            .spacing(20)
            .build();
        
        let label = Label::new(Some("Muse Shell"));
        label.add_css_class("bar-label");
        left_section.append(&label);

        // Right section - system tiles
        let right_section = Box::builder()
            .orientation(Orientation::Horizontal)
            .spacing(8)
            .halign(gtk4::Align::End)
            .hexpand(true)
            .build();

        let brightness = BrightnessWidget::new();
        let volume = VolumeWidget::new();
        let battery = BatteryWidget::new();

        right_section.append(brightness.widget());
        right_section.append(volume.widget());
        right_section.append(battery.widget());

        container.append(&left_section);
        container.append(&right_section);

        Self {
            container,
            _monitor: monitor.clone(),
        }
    }

    pub fn widget(&self) -> &Box {
        &self.container
    }
}