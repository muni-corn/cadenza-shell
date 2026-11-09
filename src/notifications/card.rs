mod image;

use gtk4::prelude::*;
use relm4::prelude::*;

use crate::notifications::{
    image::NotificationImage,
    markup,
    types::{DEFAULT_ACTION, InvokedAction, Notification, NotificationUrgency},
};

#[derive(Debug)]
pub struct NotificationCard {
    notification: Notification,
    body: markup::Body,
}

#[derive(Debug)]
pub enum NotificationCardMsg {
    Dismiss,
    Action(String), // action_id
}

#[derive(Debug)]
pub enum NotificationCardOutput {
    Dismiss(u32), // notification_id
    Action(InvokedAction),
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
                add_css_class: "background",
                set_orientation: gtk4::Orientation::Vertical,

                // Header with app info and close button
                gtk4::Box {
                    add_css_class: "header",

                    // app icon, if there is one
                    #[name = "app_icon"]
                    gtk4::Image {
                        add_css_class: "app-icon",
                    },

                    // App name
                    gtk4::Label {
                        set_text: &self.notification.app_name,
                        add_css_class: "app-name",
                        set_halign: gtk4::Align::Start,
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
                        add_css_class: "close-button",
                        connect_clicked[sender] => move |_| {
                            sender.input(NotificationCardMsg::Dismiss);
                        },

                        gtk4::Image {
                            set_icon_name: Some("window-close-symbolic"),
                        },
                    },
                },

                // image and text; a box rather than a button, since a button
                // holds a single child and would swallow clicks on links
                #[name = "content_container"]
                gtk4::Box {
                    add_css_class: "content",
                    set_hexpand: true,

                    // notification image, if there is one
                    #[name = "image"]
                    gtk4::Image {
                        set_valign: gtk4::Align::Start,
                        add_css_class: if matches!(self.notification.image, Some(NotificationImage::Icon(_))) { "icon-image" } else { "image" },
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

                        // body, rendered from the client's markup
                        #[name = "body"]
                        gtk4::Label {
                            set_markup: &self.body.markup,
                            add_css_class: "body",
                            set_wrap: true,
                            set_halign: gtk4::Align::Start,
                            set_xalign: 0.0,
                            set_lines: 4,
                            set_ellipsize: gtk4::pango::EllipsizeMode::End,
                            set_max_width_chars: 1,
                            set_hexpand: true,
                            set_visible: !self.body.markup.is_empty(),
                        },

                        // images embedded in the body
                        #[name = "body_images"]
                        gtk4::Box {
                            set_orientation: gtk4::Orientation::Vertical,
                            set_spacing: 4,
                            set_visible: !self.body.images.is_empty(),
                        },
                    },
                },

                // buttons for every action except the default one
                #[name = "actions_box"]
                gtk4::Box {
                    add_css_class: "actions",
                    #[watch]
                    set_visible: self.button_actions().next().is_some(),
                },
            }
        }
    }

    fn init_model(
        notification: Self::Init,
        _index: &Self::Index,
        _sender: FactorySender<Self>,
    ) -> Self {
        let body = markup::parse_body(&notification.body);
        Self { notification, body }
    }

    fn init_widgets(
        &mut self,
        _index: &DynamicIndex,
        root: Self::Root,
        _returned_widget: &gtk4::Widget,
        sender: FactorySender<Self>,
    ) -> Self::Widgets {
        let widgets = view_output!();

        image::show(&widgets.app_icon, self.header_icon().as_ref());
        image::show(&widgets.image, self.notification.image.as_ref());
        for body_image in &self.body.images {
            widgets.body_images.append(&image::body_image(body_image));
        }

        // clicking the notification itself invokes the "default" action
        if self.has_default_action() {
            let click = gtk4::GestureClick::new();
            let sender = sender.clone();
            click.connect_released(move |_, _, _, _| {
                sender.input(NotificationCardMsg::Action(DEFAULT_ACTION.to_string()));
            });
            widgets.content_container.add_controller(click);
        }

        widgets.body.connect_activate_link(|_, uri| {
            open_link(uri);
            glib::Propagation::Stop
        });

        // every other action gets a button
        for (action_id, label) in self.button_actions() {
            let action_button = gtk4::Button::builder().hexpand(true).build();

            // with action-icons the id names an icon and the label becomes a
            // tooltip, unless the theme lacks the icon
            if self.notification.action_icons && image::has_icon(action_id) {
                action_button.set_icon_name(action_id);
                action_button.set_tooltip_text(Some(label));
            } else {
                let button_label = gtk4::Label::builder()
                    .label(label)
                    .halign(gtk4::Align::Center)
                    .hexpand(true)
                    .build();
                action_button.set_child(Some(&button_label));
            }

            let sender = sender.clone();
            let action_id = action_id.to_string();
            action_button.connect_clicked(move |_| {
                sender.input(NotificationCardMsg::Action(action_id.clone()));
            });

            widgets.actions_box.append(&action_button);
        }

        widgets
    }

    fn update(&mut self, msg: Self::Input, sender: FactorySender<Self>) {
        match msg {
            NotificationCardMsg::Dismiss => {
                tracing::trace!(
                    notification.id = self.notification.id,
                    "notification card dismiss clicked"
                );
                let _ = sender.output(NotificationCardOutput::Dismiss(self.notification.id));
            }
            NotificationCardMsg::Action(action_id) => {
                tracing::trace!(
                    notification.id = self.notification.id,
                    %action_id,
                    "notification card action clicked"
                );
                // tokens are tied to the input event, so request one now on
                // the gtk thread rather than later in the service
                let _ = sender.output(NotificationCardOutput::Action(InvokedAction {
                    id: self.notification.id,
                    key: action_id,
                    activation_token: activation_token(),
                }));
            }
        }
    }
}

impl NotificationCard {
    fn has_default_action(&self) -> bool {
        self.notification
            .actions
            .iter()
            .any(|(id, _)| id == DEFAULT_ACTION)
    }

    /// Actions shown as buttons: everything except the default action.
    fn button_actions(&self) -> impl Iterator<Item = (&str, &str)> {
        self.notification
            .actions
            .iter()
            .filter(|(id, _)| id != DEFAULT_ACTION)
            .map(|(id, label)| (id.as_str(), label.as_str()))
    }

    fn get_urgency_class(&self) -> &'static str {
        match self.notification.urgency {
            NotificationUrgency::Low => "low",
            NotificationUrgency::Normal => "normal",
            NotificationUrgency::Critical => "critical",
        }
    }

    /// The app icon from the notification, or the icon of its desktop
    /// entry as a fallback.
    fn header_icon(&self) -> Option<NotificationImage> {
        NotificationImage::from_path_or_icon(&self.notification.app_icon).or_else(|| {
            image::has_icon(&self.notification.desktop_entry)
                .then(|| NotificationImage::Icon(self.notification.desktop_entry.clone()))
        })
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

/// Opens a link from a notification body in the default handler.
fn open_link(uri: &str) {
    let uri = uri.to_string();
    gtk4::UriLauncher::new(&uri).launch(
        None::<&gtk4::Window>,
        None::<&gio::Cancellable>,
        move |result| {
            if let Err(e) = result {
                tracing::warn!("couldn't open {uri} from a notification: {e}");
            }
        },
    );
}

/// Asks the compositor for an activation token, which lets the app that
/// handles an action raise its window.
fn activation_token() -> Option<String> {
    let display = gtk4::gdk::Display::default()?;
    display
        .app_launch_context()
        .startup_notify_id(None::<&gio::AppInfo>, &[])
        .map(String::from)
}
