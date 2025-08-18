use gdk4::Monitor;
use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, Layer, LayerShell};
use relm4::prelude::*;
use relm4::factory::FactoryVecDeque;

use crate::services::notifications::Notification;

#[derive(Debug, Clone)]
pub struct NotificationData {
    pub id: u32,
    pub app_name: String,
    pub summary: String,
    pub body: String,
    pub urgency: NotificationUrgency,
    pub timestamp: i64,
    pub actions: Vec<String>, // action_ids
}

#[derive(Debug, Clone)]
pub enum NotificationUrgency {
    Low,
    Normal,
    Critical,
}

impl From<&Notification> for NotificationData {
    fn from(notification: &Notification) -> Self {
        Self {
            id: notification.id,
            app_name: notification.app_name.clone(),
            summary: notification.summary.clone(),
            body: notification.body.clone(),
            urgency: NotificationUrgency::Normal, // TODO: Map from notification
            timestamp: notification.timestamp,
            actions: notification.actions.clone(),
        }
    }
}

#[derive(Debug)]
struct NotificationCenter {
    visible: bool,
    notifications: FactoryVecDeque<NotificationCardWidget>,
    monitor: Monitor,
}

#[derive(Debug)]
pub enum NotificationCenterMsg {
    Toggle,
    SetVisible(bool),
    UpdateNotifications(Vec<NotificationData>),
    NotificationAction(u32, String), // notification_id, action_id
    DismissNotification(u32),        // notification_id
}

#[derive(Debug)]
pub enum NotificationCenterOutput {
    NotificationDismissed(u32),
    NotificationActionTriggered(u32, String),
}

#[relm4::component(pub)]
impl SimpleComponent for NotificationCenter {
    type Init = Monitor;
    type Input = NotificationCenterMsg;
    type Output = NotificationCenterOutput;

    view! {
        #[root]
        window = gtk::ApplicationWindow {
            set_title: Some("Muse Shell Notification Center"),
            #[watch]
            set_visible: model.visible,

            // Layer shell setup is done in init

            #[name = "main_container"]
            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 32,
                set_width_request: 400,
                set_margin_top: 16,
                set_margin_bottom: 16,
                set_margin_start: 16,
                set_margin_end: 16,

                // Header section
                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    add_css_class: "notification-center-header",
                    set_hexpand: true,

                    // Clock section
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_halign: gtk::Align::Start,
                        set_hexpand: true,

                        gtk::Label {
                            set_markup: "<span size='x-large' weight='bold'>12:00</span>",
                            add_css_class: "digital-time",
                            set_halign: gtk::Align::Start,
                        },

                        gtk::Label {
                            set_text: "Today",
                            add_css_class: "digital-date",
                            set_halign: gtk::Align::Start,
                        },
                    },

                    // Calendar section
                    gtk::Calendar {
                        set_halign: gtk::Align::End,
                        set_valign: gtk::Align::Start,
                        add_css_class: "notification-calendar",
                    },
                },

                // Controls section
                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 8,
                    set_hexpand: true,

                    gtk::Label {
                        set_text: "Notifications",
                        add_css_class: "section-title",
                        set_halign: gtk::Align::Start,
                        set_hexpand: true,
                    },

                    gtk::Button {
                        set_label: "Clear All",
                        add_css_class: "clear-button",
                        set_halign: gtk::Align::End,
                        connect_clicked => move |_| {
                            // TODO: Implement clear all functionality
                            log::debug!("Clear all notifications clicked");
                        },
                    },
                },

                // Notifications content
                gtk::ScrolledWindow {
                    set_policy: (gtk::PolicyType::Never, gtk::PolicyType::Automatic),
                    set_hexpand: true,
                    set_vexpand: true,
                    set_max_content_height: 600,

                    #[local_ref]
                    notifications_box -> gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 8,
                    },
                },
            }
        }
    }

    fn init(
        monitor: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let notifications = FactoryVecDeque::builder()
            .launch(gtk::Box::default())
            .forward(sender.input_sender(), |output| match output {
                NotificationCardOutput::Dismiss(id) => NotificationCenterMsg::DismissNotification(id),
                NotificationCardOutput::Action(id, action) => NotificationCenterMsg::NotificationAction(id, action),
            });

        let model = NotificationCenter {
            visible: false,
            notifications,
            monitor: monitor.clone(),
        };

        let notifications_box = model.notifications.widget();
        let widgets = view_output!();

        // Configure layer shell after window creation
        widgets.window.init_layer_shell();
        widgets.window.set_layer(Layer::Overlay);
        widgets.window.set_exclusive_zone(-1);
        widgets.window.set_anchor(Edge::Top, true);
        widgets.window.set_anchor(Edge::Right, true);
        widgets.window.set_anchor(Edge::Bottom, true);
        widgets.window.set_monitor(Some(&monitor));
        widgets.window.set_margin(Edge::Top, 8);
        widgets.window.set_margin(Edge::Right, 8);
        widgets.window.set_margin(Edge::Bottom, 8);

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            NotificationCenterMsg::Toggle => {
                self.visible = !self.visible;
            }
            NotificationCenterMsg::SetVisible(visible) => {
                self.visible = visible;
            }
            NotificationCenterMsg::UpdateNotifications(notifications) => {
                let mut guard = self.notifications.guard();
                guard.clear();

                // Sort notifications by timestamp (newest first)
                let mut sorted_notifications = notifications;
                sorted_notifications.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

                for notification in sorted_notifications {
                    guard.push_back(notification);
                }
            }
            NotificationCenterMsg::DismissNotification(id) => {
                let _ = sender.output(NotificationCenterOutput::NotificationDismissed(id));
            }
            NotificationCenterMsg::NotificationAction(id, action) => {
                let _ = sender.output(NotificationCenterOutput::NotificationActionTriggered(
                    id, action,
                ));
            }
        }
    }
}

// Factory for individual notification cards
#[derive(Debug)]
struct NotificationCardWidget {
    notification: NotificationData,
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

#[relm4::factory]
impl FactoryComponent for NotificationCardWidget {
    type Init = NotificationData;
    type Input = NotificationCardMsg;
    type Output = NotificationCardOutput;
    type CommandOutput = ();
    type ParentWidget = gtk::Box;

    view! {
        #[root]
        card = gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 8,
            add_css_class: "notification-card",
            add_css_class: &self.get_urgency_class(),

            // Header with app name and close button
            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 8,

                gtk::Label {
                    #[watch]
                    set_text: &self.notification.app_name,
                    add_css_class: "notification-app-name",
                    set_halign: gtk::Align::Start,
                    set_hexpand: true,
                },

                gtk::Label {
                    #[watch]
                    set_text: &self.format_time(self.notification.timestamp),
                    add_css_class: "notification-time",
                },

                gtk::Button {
                    set_icon_name: "window-close-symbolic",
                    add_css_class: "notification-close",
                    connect_clicked[sender] => move |_| {
                        sender.input(NotificationCardMsg::Dismiss);
                    },
                },
            },

            // Content
            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 4,

                gtk::Label {
                    #[watch]
                    set_markup: &format!("<b>{}</b>", glib::markup_escape_text(&self.notification.summary)),
                    add_css_class: "notification-summary",
                    set_halign: gtk::Align::Start,
                    set_wrap: true,
                },

                gtk::Label {
                    #[watch]
                    set_text: &self.notification.body,
                    add_css_class: "notification-body",
                    set_halign: gtk::Align::Start,
                    set_wrap: true,
                    set_lines: 3,
                    set_ellipsize: gtk::pango::EllipsizeMode::End,
                    #[watch]
                    set_visible: !self.notification.body.is_empty(),
                },
            },

            // Actions (if any)
            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 8,
                set_halign: gtk::Align::End,
                #[watch]
                set_visible: !self.notification.actions.is_empty(),

                // Create buttons for each action dynamically
                // Note: This is simplified - in a real implementation you'd want a nested factory
            },
        }
    }

    fn init_model(
        notification: Self::Init,
        _index: &DynamicIndex,
        _sender: FactorySender<Self>,
    ) -> Self {
        Self { notification }
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

impl NotificationCardWidget {
    fn get_urgency_class(&self) -> String {
        match self.notification.urgency {
            NotificationUrgency::Low => "notification-low".to_string(),
            NotificationUrgency::Normal => "notification-normal".to_string(),
            NotificationUrgency::Critical => "notification-critical".to_string(),
        }
    }

    fn format_time(&self, timestamp: i64) -> String {
        use chrono::{DateTime, Local, Utc};

        let datetime = DateTime::from_timestamp(timestamp, 0)
            .unwrap_or_else(|| Utc::now())
            .with_timezone(&Local);

        let now = Local::now();
        let duration = now.signed_duration_since(datetime);

        if duration.num_minutes() < 1 {
            "now".to_string()
        } else if duration.num_minutes() < 60 {
            format!("{}m", duration.num_minutes())
        } else if duration.num_hours() < 24 {
            format!("{}h", duration.num_hours())
        } else {
            format!("{}d", duration.num_days())
        }
    }
}

pub fn create_notification_center(monitor: Monitor) -> Controller<NotificationCenter> {
    NotificationCenter::builder().launch(monitor).detach()
}
