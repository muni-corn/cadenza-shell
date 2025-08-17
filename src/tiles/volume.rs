use crate::services::audio::AudioService;
use crate::utils::icons::{MUTE_ICON, VOLUME_ICONS};
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{Box, Label, Orientation, ProgressBar};

pub struct VolumeWidget {
    container: Box,
    service: AudioService,
}

impl VolumeWidget {
    pub fn new() -> Self {
        let container = Box::builder()
            .orientation(Orientation::Horizontal)
            .spacing(8)
            .css_classes(vec!["tile"])
            .build();

        let service = AudioService::new();

        // Only show widget if audio is available
        service
            .bind_property("available", &container, "visible")
            .sync_create()
            .build();

        if service.available() {
            let icon_label = Label::builder()
                .css_classes(vec!["icon", "dim"])
                .width_request(16)
                .build();

            let progress_bar = ProgressBar::builder()
                .css_classes(vec!["dim"])
                .valign(gtk4::Align::Center)
                .width_request(16)
                .build();

            // Bind volume to progress bar
            service
                .bind_property("volume", &progress_bar, "fraction")
                .sync_create()
                .build();

            // Update icon when either volume or mute changes
            let update_icon = glib::clone!(@weak icon_label, @weak service, @weak progress_bar => move || {
                let icon = if service.muted() {
                    MUTE_ICON
                } else {
                    let volume = service.volume();
                    let idx = if volume == 0.0 {
                        0 // Silent
                    } else if volume < 0.5 {
                        1 // Low
                    } else {
                        2 // High
                    };
                    VOLUME_ICONS[idx]
                };
                icon_label.set_text(icon);

                // Trigger fade animation
                icon_label.remove_css_class("dim");
                icon_label.add_css_class("bright");
                progress_bar.remove_css_class("dim");
                progress_bar.add_css_class("bright");

                glib::timeout_add_local_once(std::time::Duration::from_secs(3),
                    glib::clone!(@weak icon_label, @weak progress_bar => move || {
                        icon_label.remove_css_class("bright");
                        icon_label.add_css_class("dim");
                        progress_bar.remove_css_class("bright");
                        progress_bar.add_css_class("dim");
                    })
                );
            });

            service.connect_volume_notify(glib::clone!(@strong update_icon => move |_| {
                update_icon();
            }));

            service.connect_muted_notify(glib::clone!(@strong update_icon => move |_| {
                update_icon();
            }));

            // Initial icon update
            update_icon();

            container.append(&icon_label);
            container.append(&progress_bar);
        }

        Self { container, service }
    }

    pub fn widget(&self) -> &Box {
        &self.container
    }

    pub fn service(&self) -> &AudioService {
        &self.service
    }
}
