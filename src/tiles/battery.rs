use crate::services::battery::BatteryService;
use crate::utils::icons::{percentage_to_icon_from_list, BATTERY_CHARGING_ICONS, BATTERY_ICONS};
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{Box, Label, Orientation, ProgressBar};

pub struct BatteryWidget {
    container: Box,
    service: BatteryService,
}

impl BatteryWidget {
    pub fn new() -> Self {
        let container = Box::builder()
            .orientation(Orientation::Horizontal)
            .spacing(8)
            .css_classes(vec!["tile"])
            .build();

        let service = BatteryService::new();

        // Only show widget if battery is available
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

            let percentage_label = Label::builder()
                .css_classes(vec!["percentage", "dim"])
                .width_request(30)
                .build();

            // Bind percentage to progress bar
            service
                .bind_property("percentage", &progress_bar, "fraction")
                .sync_create()
                .build();

            // Update icon, percentage text, and styling based on battery state
            let update_display = glib::clone!(
                #[weak]
                icon_label,
                #[weak]
                service,
                #[weak]
                progress_bar,
                #[weak]
                percentage_label,
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
                    icon_label.set_text(icon);

                    // Update percentage text
                    percentage_label.set_text(&format!("{}%", (percentage * 100.0) as u32));

                    // Update CSS classes based on battery state
                    icon_label.remove_css_class("battery-low");
                    icon_label.remove_css_class("battery-critical");
                    icon_label.remove_css_class("battery-charging");
                    progress_bar.remove_css_class("battery-low");
                    progress_bar.remove_css_class("battery-critical");
                    progress_bar.remove_css_class("battery-charging");
                    percentage_label.remove_css_class("battery-low");
                    percentage_label.remove_css_class("battery-critical");
                    percentage_label.remove_css_class("battery-charging");

                    if is_charging {
                        icon_label.add_css_class("battery-charging");
                        progress_bar.add_css_class("battery-charging");
                        percentage_label.add_css_class("battery-charging");
                    } else if is_critical {
                        icon_label.add_css_class("battery-critical");
                        progress_bar.add_css_class("battery-critical");
                        percentage_label.add_css_class("battery-critical");
                    } else if is_low {
                        icon_label.add_css_class("battery-low");
                        progress_bar.add_css_class("battery-low");
                        percentage_label.add_css_class("battery-low");
                    }

                    // Trigger fade animation when values change
                    icon_label.remove_css_class("dim");
                    icon_label.add_css_class("bright");
                    progress_bar.remove_css_class("dim");
                    progress_bar.add_css_class("bright");
                    percentage_label.remove_css_class("dim");
                    percentage_label.add_css_class("bright");

                    glib::timeout_add_local_once(
                        std::time::Duration::from_secs(3),
                        glib::clone!(
                            #[weak]
                            icon_label,
                            #[weak]
                            progress_bar,
                            #[weak]
                            percentage_label,
                            move || {
                                icon_label.remove_css_class("bright");
                                icon_label.add_css_class("dim");
                                progress_bar.remove_css_class("bright");
                                progress_bar.add_css_class("dim");
                                percentage_label.remove_css_class("bright");
                                percentage_label.add_css_class("dim");
                            }
                        ),
                    );
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

            // Initial display update
            update_display();

            container.append(&icon_label);
            container.append(&progress_bar);
            container.append(&percentage_label);
        }

        Self { container, service }
    }

    pub fn widget(&self) -> &Box {
        &self.container
    }

    pub fn service(&self) -> &BatteryService {
        &self.service
    }
}
