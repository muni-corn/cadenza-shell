use crate::services::brightness::BrightnessService;
use crate::utils::icons::{BRIGHTNESS_ICONS, percentage_to_icon_from_list};
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{Box, Label, Orientation, ProgressBar};

pub struct BrightnessWidget {
    container: Box,
    _service: BrightnessService,
}

impl BrightnessWidget {
    pub fn new() -> Self {
        let container = Box::builder()
            .orientation(Orientation::Horizontal)
            .spacing(8)
            .css_classes(vec!["tile"])
            .build();

        let service = BrightnessService::new();

        // Only show widget if brightness is available
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

            // Bind brightness value directly to progress bar
            service
                .bind_property("brightness", &progress_bar, "fraction")
                .sync_create()
                .build();

            // Update icon based on brightness changes
            service.connect_brightness_notify(glib::clone!(
                #[weak]
                icon_label,
                move |service| {
                    let brightness = service.brightness();
                    let icon = percentage_to_icon_from_list(brightness, BRIGHTNESS_ICONS);
                    icon_label.set_text(icon);

                    // Trigger fade animation
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
            ));

            // Similar animation for progress bar
            service.connect_brightness_notify(glib::clone!(
                #[weak]
                progress_bar,
                move |_| {
                    progress_bar.remove_css_class("dim");
                    progress_bar.add_css_class("bright");

                    glib::timeout_add_local_once(
                        std::time::Duration::from_secs(3),
                        glib::clone!(
                            #[weak]
                            progress_bar,
                            move || {
                                progress_bar.remove_css_class("bright");
                                progress_bar.add_css_class("dim");
                            }
                        ),
                    );
                }
            ));

            container.append(&icon_label);
            container.append(&progress_bar);
        }

        Self {
            container,
            _service: service,
        }
    }

    pub fn widget(&self) -> &Box {
        &self.container
    }
}