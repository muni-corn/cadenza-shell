use gtk4::prelude::*;
use relm4::prelude::*;
use std::path::Path;

use crate::services::notifications::Notification;

#[derive(Debug, Clone)]
pub struct NotificationData {
    pub id: u32,
    pub app_name: String,
    pub app_icon: String,
    pub summary: String,
    pub body: String,
    pub urgency: u8,
    pub timestamp: i64,
    pub actions: Vec<String>,
}

impl From<&Notification> for NotificationData {
    fn from(notification: &Notification) -> Self {
        Self {
            id: notification.id,
            app_name: notification.app_name.clone(),
            app_icon: notification.app_icon.clone(),
            summary: notification.summary.clone(),
            body: notification.body.clone(),
            urgency: notification.urgency,
            timestamp: notification.timestamp,
            actions: notification.actions.clone(),
        }
    }
}

impl From<Notification> for NotificationData {
    fn from(notification: Notification) -> Self {
        Self {
            id: notification.id,
            app_name: notification.app_name,
            app_icon: notification.app_icon,
            summary: notification.summary,
            body: notification.body,
            urgency: notification.urgency,
            timestamp: notification.timestamp,
            actions: notification.actions,
        }
    }
}

#[derive(Debug)]
pub struct NotificationCard {
    notification: NotificationData,
}

#[derive(Debug)]
pub enum NotificationCardMsg {
    Dismiss,
    Action(String), // action_id
    DefaultAction,
}

#[derive(Debug)]
pub enum NotificationCardOutput {
    Dismiss(u32),        // notification_id
    Action(u32, String), // notification_id, action_id
    DefaultAction(u32),  // notification_id
}

#[relm4::factory(pub)]
impl FactoryComponent for NotificationCard {
    type Init = NotificationData;
    type Input = NotificationCardMsg;
    type Output = NotificationCardOutput;
    type CommandOutput = ();
    type ParentWidget = gtk4::Box;

    view! {
        #[root]
        card = gtk4::Box {
            set_orientation: gtk4::Orientation::Vertical,
            set_spacing: 8,
            set_margin_top: 8,
            set_margin_bottom: 8,
            set_margin_start: 12,
            set_margin_end: 12,
            add_css_class: "notification-card",
            add_css_class: &self.get_urgency_class(),

            // Header with app info and close button
            gtk4::Box {
                set_orientation: gtk4::Orientation::Horizontal,
                set_spacing: 8,
                add_css_class: "header",

                // App icon (if available)
                gtk4::Box {
                    set_orientation: gtk4::Orientation::Horizontal,
                    set_spacing: 8,
                    #[watch]
                    set_visible: !self.notification.app_icon.is_empty(),

                    gtk4::Image {
                        #[watch]
                        set_icon_name: if self.is_icon_file() {
                            None
                        } else {
                            Some(&self.notification.app_icon)
                        },
                        #[watch]
                        set_from_file: if self.is_icon_file() {
                            Some(&self.notification.app_icon)
                        } else {
                            None
                        },
                        set_pixel_size: 16,
                        add_css_class: "app-icon",
                    },
                },

                // App name
                gtk4::Label {
                    #[watch]
                    set_text: &self.notification.app_name,
                    add_css_class: "app-name",
                    set_halign: gtk4::Align::Start,
                    #[watch]
                    set_visible: !self.notification.app_name.is_empty(),
                },

                // Spacer
                gtk4::Box {
                    set_hexpand: true,
                },

                // Time
                gtk4::Label {
                    #[watch]
                    set_text: &self.format_time(self.notification.timestamp),
                    add_css_class: "time",
                    set_halign: gtk4::Align::End,
                },

                // Close button
                gtk4::Button {
                    set_icon_name: "window-close-symbolic",
                    add_css_class: "close-button",
                    connect_clicked[sender] => move |_| {
                        sender.input(NotificationCardMsg::Dismiss);
                    },

                    gtk4::Image {
                        set_icon_name: Some("window-close-symbolic"),
                        set_pixel_size: 12,
                    },
                },
            },

            // Content section
            gtk4::Box {
                set_orientation: gtk4::Orientation::Horizontal,
                set_spacing: 12,
                add_css_class: "content",

                // Notification image (if different from app icon and is a file)
                gtk4::Image {
                    #[watch]
                    set_visible: self.has_notification_image(),
                    #[watch]
                    set_from_file: if self.has_notification_image() {
                        Some(&self.notification.app_icon)
                    } else {
                        None
                    },
                    set_pixel_size: 48,
                    set_valign: gtk4::Align::Start,
                    add_css_class: "image",
                },

                // Text content
                gtk4::Box {
                    set_orientation: gtk4::Orientation::Vertical,
                    set_spacing: 4,
                    set_hexpand: true,

                    // Summary
                    gtk4::Label {
                        #[watch]
                        set_text: &self.notification.summary,
                        add_css_class: "summary",
                        set_halign: gtk4::Align::Start,
                        set_xalign: 0.0,
                        set_wrap: true,
                        set_lines: 2,
                        set_ellipsize: gtk4::pango::EllipsizeMode::End,
                        set_max_width_chars: 50,
                        set_hexpand: true,
                    },

                    // Body (if present)
                    gtk4::Label {
                        #[watch]
                        set_text: &self.notification.body,
                        add_css_class: "body",
                        set_halign: gtk4::Align::Start,
                        set_xalign: 0.0,
                        set_wrap: true,
                        set_lines: 4,
                        set_ellipsize: gtk4::pango::EllipsizeMode::End,
                        set_max_width_chars: 50,
                        set_hexpand: true,
                        #[watch]
                        set_visible: !self.notification.body.is_empty(),
                    },
                },
            },

            // Actions section (if multiple actions)
            gtk4::Box {
                set_orientation: gtk4::Orientation::Horizontal,
                set_spacing: 8,
                set_homogeneous: true,
                add_css_class: "actions",
                #[watch]
                set_visible: self.notification.actions.len() > 1,

                // TODO: Implement dynamic action buttons
                // For now, just show a placeholder for multiple actions
                gtk4::Label {
                    set_text: "Actions available",
                    add_css_class: "actions-placeholder",
                },
            },
        }
    }

    fn init_model(
        notification: Self::Init,
        _index: &DynamicIndex,
        _sender: FactorySender<Self>,
    ) -> Self {
        let card = Self { notification };

        // If there's exactly one action, make the whole card clickable
        if card.notification.actions.len() == 1 {
            // We'll handle this by connecting to the content_container click
        }

        card
    }

    fn update(&mut self, msg: Self::Input, sender: FactorySender<Self>) {
        match msg {
            NotificationCardMsg::Dismiss => {
                let _ = sender.output(NotificationCardOutput::Dismiss(self.notification.id));
            }
            NotificationCardMsg::Action(action_id) => {
                let _ = sender.output(NotificationCardOutput::Action(
                    self.notification.id,
                    action_id,
                ));
            }
            NotificationCardMsg::DefaultAction => {
                let _ = sender.output(NotificationCardOutput::DefaultAction(self.notification.id));
            }
        }
    }
}

impl NotificationCard {
    fn get_urgency_class(&self) -> &'static str {
        match self.notification.urgency {
            0 => "low",
            2 => "critical",
            _ => "normal",
        }
    }

    fn is_icon_file(&self) -> bool {
        !self.notification.app_icon.is_empty() && Path::new(&self.notification.app_icon).exists()
    }

    fn has_notification_image(&self) -> bool {
        self.is_icon_file() && !self.notification.app_icon.is_empty()
    }

    fn format_time(&self, timestamp: i64) -> String {
        use chrono::{DateTime, Local, Utc};

        let datetime = DateTime::from_timestamp(timestamp, 0)
            .unwrap_or_else(|| Utc::now())
            .with_timezone(&Local);

        datetime.format("%-I:%M %P").to_string()
    }

    pub fn notification_id(&self) -> u32 {
        self.notification.id
    }
}

// Export the factory component for use in other modules
pub type NotificationCardFactory = NotificationCard;
