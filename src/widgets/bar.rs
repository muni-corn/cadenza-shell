use gtk4::prelude::*;
use gtk4::{Box, Orientation, Label};
use gdk4::Monitor;

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

        // For now, just add a simple label
        let label = Label::new(Some("Muse Shell"));
        label.add_css_class("bar-label");
        container.append(&label);

        Self {
            container,
            _monitor: monitor.clone(),
        }
    }

    pub fn widget(&self) -> &Box {
        &self.container
    }
}