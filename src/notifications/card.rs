use std::path::Path;

use glib::clone;
use gtk4::prelude::*;
use relm4::prelude::*;

use crate::notifications::types::{Notification, NotificationUrgency};

fn is_icon(icon: &str) -> bool {
    if let Some(display) = gtk4::gdk::Display::default() {
        let icon_theme = gtk4::IconTheme::for_display(&display);
        !icon.is_empty() && icon_theme.has_icon(icon)
    } else {
        false
    }
}

fn file_exists(path: &str) -> bool {
    !path.is_empty() && Path::new(path).exists()
}

#[derive(Debug)]
pub struct NotificationCard {
    notification: Notification,
}

#[derive(Debug)]
pub enum NotificationCardMsg {
    Dismiss,
    Action(String), // action_id
}

#[derive(Debug)]
pub enum NotificationCardOutput {
    Dismiss(u32),        // notification_id
    Action(u32, String), // notification_id, action_id
}

#[relm4::factory(pub)]
impl FactoryComponent for NotificationCard {
    type CommandOutput = ();
    type Init = Notification;
    type Input = NotificationCardMsg;
    type Output = NotificationCardOutput;
    type ParentWidget = gtk4::Box;

    view! {
        #[root]
        card = gtk4::Box {
            add_css_class: "notification-card",
            add_css_class: self.get_urgency_class(),

            gtk4::Box {
                set_orientation: gtk4::Orientation::Vertical,

                // Header with app info and close button
                gtk4::Box {
                    add_css_class: "header",

                    // App icon (if available)
                    gtk4::Image {
                        #[watch]
                        set_visible: self.has_app_icon(),
                        #[watch]
                        set_icon_name: self.get_app_icon_name().as_deref(),
                        add_css_class: "app-icon",
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

                    // Time
                    gtk4::Label {
                        #[watch]
                        set_text: &self.format_time(self.notification.timestamp),
                        add_css_class: "time",
                        set_hexpand: true,
                        set_halign: gtk4::Align::End,
                    },

                    // Close button
                    gtk4::Button {
                        add_css_class: "closeButton",
                        connect_clicked[sender] => move |_| {
                            sender.input(NotificationCardMsg::Dismiss);
                        },

                        gtk4::Image {
                            set_icon_name: Some("window-close-symbolic"),
                        },
                    },
                },

                // Content section
                #[name = "content_container"]
                gtk4::Box {
                    add_css_class: "content",
                    set_hexpand: true,

                    // Notification image (if available)
                    gtk4::Image {
                        #[watch]
                        set_visible: self.has_notification_image(),
                        #[watch]
                        set_from_file: if self.has_notification_image() && file_exists(&self.notification.image) {
                            Some(&self.notification.image)
                        } else {
                            None
                        },
                        #[watch]
                        set_icon_name: if self.has_notification_image() && !file_exists(&self.notification.image) {
                            Some(&self.notification.image)
                        } else {
                            None
                        },
                        set_valign: gtk4::Align::Start,
                        add_css_class: if file_exists(&self.notification.image) { "image" } else { "icon-image" },
                    },

                    // Text content
                    gtk4::Box {
                        set_orientation: gtk4::Orientation::Vertical,

                        // Summary
                        gtk4::Label {
                            #[watch]
                            set_text: &self.notification.summary,
                            add_css_class: "summary",
                            set_wrap: true,
                            set_halign: gtk4::Align::Start,
                            set_xalign: 0.0,
                            set_lines: 2,
                            set_ellipsize: gtk4::pango::EllipsizeMode::End,
                            set_max_width_chars: 1,
                            set_hexpand: true,
                        },

                        // Body (if present)
                        gtk4::Label {
                            #[watch]
                            set_text: &self.notification.body,
                            add_css_class: "body",
                            set_wrap: true,
                            set_halign: gtk4::Align::Start,
                            set_xalign: 0.0,
                            set_lines: 4,
                            set_ellipsize: gtk4::pango::EllipsizeMode::End,
                            set_max_width_chars: 1,
                            set_hexpand: true,
                            #[watch]
                            set_visible: !self.notification.body.is_empty(),
                        },
                    },
                },

                // Actions section (if multiple actions)
                #[name = "actions_box"]
                gtk4::Box {
                    add_css_class: "actions",
                    #[watch]
                    set_visible: self.notification.actions.len() > 1,
                },
            }
        }
    }

    fn init_model(
        notification: Self::Init,
        _index: &Self::Index,
        _sender: FactorySender<Self>,
    ) -> Self {
        Self { notification }
    }

    fn init_widgets(
        &mut self,
        _index: &DynamicIndex,
        root: Self::Root,
        _returned_widget: &gtk4::Widget,
        sender: FactorySender<Self>,
    ) -> Self::Widgets {
        let widgets = view_output!();

        // If exactly one action, wrap content in clickable button
        if self.notification.actions.len() == 1 {
            let action_id = self.notification.actions[0].clone();
            let button = gtk4::Button::builder()
                .child(&widgets.content_container)
                .build();

            button.connect_clicked(clone!(
                #[strong]
                sender,
                #[strong]
                action_id,
                move |_| {
                    sender.input(NotificationCardMsg::Action(action_id.clone()));
                }
            ));

            // Replace content_container with the button
            if let Some(parent) = widgets.content_container.parent() {
                let parent_box = parent.downcast::<gtk4::Box>().unwrap();
                parent_box.remove(&widgets.content_container);
                parent_box.append(&button);
            }
        }

        // If multiple actions, create action buttons
        if self.notification.actions.len() > 1 {
            // Actions come in pairs: [action_id, label, action_id, label, ...]
            for chunk in self.notification.actions.chunks(2) {
                if chunk.len() == 2 {
                    let action_id = chunk[0].clone();
                    let label = chunk[1].clone();

                    let action_button = gtk4::Button::builder().hexpand(true).build();

                    let button_label = gtk4::Label::builder()
                        .label(&label)
                        .halign(gtk4::Align::Center)
                        .hexpand(true)
                        .build();

                    action_button.set_child(Some(&button_label));

                    action_button.connect_clicked(clone!(
                        #[strong]
                        sender,
                        #[strong]
                        action_id,
                        move |_| {
                            sender.input(NotificationCardMsg::Action(action_id.clone()));
                        }
                    ));

                    widgets.actions_box.append(&action_button);
                }
            }
        }

        widgets
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
        }
    }
}

impl NotificationCard {
    fn get_urgency_class(&self) -> &'static str {
        match self.notification.urgency {
            NotificationUrgency::Low => "low",
            NotificationUrgency::Normal => "normal",
            NotificationUrgency::Critical => "critical",
        }
    }

    fn has_app_icon(&self) -> bool {
        !self.notification.app_icon.is_empty() || is_icon(&self.notification.desktop_entry)
    }

    fn get_app_icon_name(&self) -> Option<String> {
        if !self.notification.app_icon.is_empty() {
            Some(self.notification.app_icon.clone())
        } else if is_icon(&self.notification.desktop_entry) {
            Some(self.notification.desktop_entry.clone())
        } else {
            None
        }
    }

    fn has_notification_image(&self) -> bool {
        !self.notification.image.is_empty()
    }

    fn format_time(&self, timestamp: i64) -> String {
        use chrono::{DateTime, Local, Utc};

        let datetime = DateTime::from_timestamp(timestamp, 0)
            .unwrap_or_else(Utc::now)
            .with_timezone(&Local);

        datetime.format("%-I:%M %P").to_string()
    }

    pub fn notification_id(&self) -> u32 {
        self.notification.id
    }
}
