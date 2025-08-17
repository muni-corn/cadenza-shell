use crate::services::notifications::Notification;
use gtk4::prelude::*;
use gtk4::{Box, Button, Image, Label, Orientation};
use gtk4::glib;
use std::path::Path;

pub struct NotificationCard {
    container: Box,
    notification: Notification,
}

impl NotificationCard {
    pub fn new(notification: Notification) -> Self {
        let container = Box::builder()
            .orientation(Orientation::Vertical)
            .css_classes(vec![
                "notification-card",
                &Self::urgency_class(&notification),
            ])
            .spacing(8)
            .margin_top(8)
            .margin_bottom(8)
            .margin_start(12)
            .margin_end(12)
            .build();

        let card = Self {
            container,
            notification: notification.clone(),
        };

        card.build_header();
        card.build_content();
        card.build_actions();

        card
    }

    fn urgency_class(notification: &Notification) -> String {
        match notification.urgency {
            0 => "low".to_string(),
            2 => "critical".to_string(),
            _ => "normal".to_string(),
        }
    }

    fn build_header(&self) {
        let header = Box::builder()
            .orientation(Orientation::Horizontal)
            .css_classes(vec!["header"])
            .spacing(8)
            .build();

        // App icon
        if !self.notification.app_icon.is_empty() {
            let app_icon = if Path::new(&self.notification.app_icon).exists() {
                Image::from_file(&self.notification.app_icon)
            } else {
                Image::from_icon_name(&self.notification.app_icon)
            };
            app_icon.set_css_classes(&["app-icon"]);
            app_icon.set_pixel_size(16);
            header.append(&app_icon);
        }

        // App name
        if !self.notification.app_name.is_empty() {
            let app_name = Label::builder()
                .label(&self.notification.app_name)
                .css_classes(vec!["app-name"])
                .halign(gtk4::Align::Start)
                .build();
            header.append(&app_name);
        }

        // Time
        let time_label = Label::builder()
            .label(&Self::format_time(self.notification.timestamp))
            .css_classes(vec!["time"])
            .halign(gtk4::Align::End)
            .hexpand(true)
            .build();
        header.append(&time_label);

        // Close button
        let close_button = Button::builder().css_classes(vec!["close-button"]).build();

        let close_icon = Image::from_icon_name("window-close-symbolic");
        close_icon.set_pixel_size(12);
        close_button.set_child(Some(&close_icon));

        let notification_id = self.notification.id;
        close_button.connect_clicked(move |_| {
            // TODO: Implement notification dismissal
            log::info!("Dismissing notification {}", notification_id);
        });

        header.append(&close_button);
        self.container.append(&header);
    }

    fn build_content(&self) {
        let content = Box::builder()
            .orientation(Orientation::Horizontal)
            .css_classes(vec!["content"])
            .spacing(12)
            .build();

        // Notification image (if any)
        if !self.notification.app_icon.is_empty() && Path::new(&self.notification.app_icon).exists()
        {
            let image = Image::from_file(&self.notification.app_icon);
            image.set_css_classes(&["image"]);
            image.set_pixel_size(48);
            image.set_valign(gtk4::Align::Start);
            content.append(&image);
        }

        // Text content
        let text_box = Box::builder()
            .orientation(Orientation::Vertical)
            .spacing(4)
            .hexpand(true)
            .build();

        // Summary
        let summary = Label::builder()
            .label(&self.notification.summary)
            .css_classes(vec!["summary"])
            .halign(gtk4::Align::Start)
            .xalign(0.0)
            .wrap(true)
            .lines(2)
            .ellipsize(gtk4::pango::EllipsizeMode::End)
            .max_width_chars(50)
            .hexpand(true)
            .build();
        text_box.append(&summary);

        // Body (if present)
        if !self.notification.body.is_empty() {
            let body = Label::builder()
                .label(&self.notification.body)
                .css_classes(vec!["body"])
                .halign(gtk4::Align::Start)
                .xalign(0.0)
                .wrap(true)
                .lines(4)
                .ellipsize(gtk4::pango::EllipsizeMode::End)
                .max_width_chars(50)
                .hexpand(true)
                .build();
            text_box.append(&body);
        }

        content.append(&text_box);

        // Wrap in button if actionable
        if !self.notification.actions.is_empty() {
            let action_button = Button::new();
            action_button.set_child(Some(&content));

            let notification_id = self.notification.id;
            action_button.connect_clicked(move |_| {
                // TODO: Implement action invocation
                log::info!(
                    "Invoking default action for notification {}",
                    notification_id
                );
            });

            self.container.append(&action_button);
        } else {
            self.container.append(&content);
        }
    }

    fn build_actions(&self) {
        if self.notification.actions.len() > 1 {
            let actions_box = Box::builder()
                .orientation(Orientation::Horizontal)
                .css_classes(vec!["actions"])
                .spacing(8)
                .homogeneous(true)
                .build();

            for action in &self.notification.actions {
                let action_button = Button::builder().label(action).hexpand(true).build();

                let action_text = action.clone();
                let notification_id = self.notification.id;
                action_button.connect_clicked(move |_| {
                    // TODO: Implement specific action invocation
                    log::info!(
                        "Invoking action '{}' for notification {}",
                        action_text,
                        notification_id
                    );
                });

                actions_box.append(&action_button);
            }

            self.container.append(&actions_box);
        }
    }

    fn format_time(timestamp: i64) -> String {
        let datetime =
            chrono::DateTime::from_timestamp(timestamp, 0).unwrap_or_else(|| chrono::Utc::now());
        let local_time = datetime.with_timezone(&chrono::Local);
        local_time.format("%-I:%M %P").to_string()
    }

    pub fn widget(&self) -> &Box {
        &self.container
    }

    pub fn notification_id(&self) -> u32 {
        self.notification.id
    }
}