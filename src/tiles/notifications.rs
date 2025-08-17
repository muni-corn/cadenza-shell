use crate::services::notifications::NotificationService;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{Box, Button, Label, Orientation};

const NOTIFICATION_ICON: &str = "󰂚";
const NOTIFICATION_NEW_ICON: &str = "󰂛";

pub struct NotificationTile {
    container: Box,
    button: Button,
    icon_label: Label,
    count_label: Label,
    service: NotificationService,
}

impl NotificationTile {
    pub fn new(service: NotificationService) -> Self {
        let container = Box::builder()
            .orientation(Orientation::Horizontal)
            .css_classes(vec!["tile"])
            .build();

        let button = Button::builder()
            .css_classes(vec!["notification-button"])
            .build();

        let button_content = Box::builder()
            .orientation(Orientation::Horizontal)
            .spacing(6)
            .build();

        let icon_label = Label::builder()
            .css_classes(vec!["icon"])
            .label(NOTIFICATION_ICON)
            .width_request(16)
            .build();

        let count_label = Label::builder()
            .css_classes(vec!["count"])
            .visible(false)
            .build();

        button_content.append(&icon_label);
        button_content.append(&count_label);
        button.set_child(Some(&button_content));
        container.append(&button);

        let tile = Self {
            container,
            button,
            icon_label,
            count_label,
            service,
        };

        tile.setup_bindings();
        tile.setup_click_handler();

        tile
    }

    fn setup_bindings(&self) {
        // Bind notification count to count label
        self.service.connect_notification_count_notify(glib::clone!(
            @weak self.count_label as count_label,
            @weak self.icon_label as icon_label => move |service| {
                let count = service.notification_count();

                if count > 0 {
                    count_label.set_text(&count.to_string());
                    count_label.set_visible(true);
                    icon_label.set_text(NOTIFICATION_NEW_ICON);
                } else {
                    count_label.set_visible(false);
                    icon_label.set_text(NOTIFICATION_ICON);
                }
            }
        ));

        // Bind has_notifications to widget visibility
        self.service
            .bind_property("has-notifications", &self.container, "visible")
            .sync_create()
            .build();
    }

    fn setup_click_handler(&self) {
        self.button.connect_clicked(move |_| {
            // TODO: Toggle notification center visibility
            // This would need to be connected to the notification center instance
            log::info!("Notification tile clicked - should toggle notification center");
        });
    }

    pub fn widget(&self) -> &Box {
        &self.container
    }

    pub fn set_click_handler<F>(&self, handler: F)
    where
        F: Fn() + 'static,
    {
        self.button.connect_clicked(move |_| {
            handler();
        });
    }
}

impl Default for NotificationTile {
    fn default() -> Self {
        Self::new(NotificationService::new())
    }
}