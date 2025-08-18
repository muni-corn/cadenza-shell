use gtk4::prelude::*;
use relm4::prelude::*;

use crate::messages::TileOutput;

const NOTIFICATION_ICON: &str = "󰂚";
const NOTIFICATION_NEW_ICON: &str = "󰂛";

#[derive(Debug)]
struct NotificationsWidget {
    notification_count: u32,
    has_notifications: bool,
}

#[derive(Debug)]
pub enum NotificationsMsg {
    Click,
    UpdateNotifications(u32), // notification count
}

#[relm4::component(pub)]
impl SimpleComponent for NotificationsWidget {
    type Init = ();
    type Input = NotificationsMsg;
    type Output = TileOutput;

    view! {
        #[root]
        tile_button = gtk::Button {
            add_css_class: "tile",
            add_css_class: "notifications",

            connect_clicked[sender] => move |_| {
                sender.input(NotificationsMsg::Click);
            },

            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 8,
                set_halign: gtk::Align::Center,

                gtk::Label {
                    #[watch]
                    set_label: &if model.notification_count > 0 { NOTIFICATION_NEW_ICON } else { NOTIFICATION_ICON },
                    add_css_class: "tile-icon",
                    set_width_request: 16,
                },

                gtk::Label {
                    #[watch]
                    set_label: &if model.notification_count > 0 {
                        model.notification_count.to_string()
                    } else {
                        "".to_string()
                    },
                    #[watch]
                    set_visible: model.notification_count > 0,
                    add_css_class: "tile-text",
                    add_css_class: "notification-count",
                },
            }
        }
    }

    fn init(
        _init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = NotificationsWidget {
            notification_count: 0,
            has_notifications: false,
        };

        let widgets = view_output!();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            NotificationsMsg::Click => {
                log::debug!("Notifications tile clicked");
                let _ = sender.output(TileOutput::Clicked("notifications".to_string()));
            }
            NotificationsMsg::UpdateNotifications(count) => {
                self.notification_count = count;
                self.has_notifications = count > 0;
            }
        }
    }
}

impl NotificationsWidget {
    fn get_notification_icon(&self) -> String {
        if self.notification_count > 0 {
            NOTIFICATION_NEW_ICON.to_string()
        } else {
            NOTIFICATION_ICON.to_string()
        }
    }
}

pub fn create_notifications_widget() -> gtk4::Widget {
    let controller = NotificationsWidget::builder().launch(()).detach();
    controller.widget().clone().into()
}