use std::collections::HashMap;

use gdk4::Monitor;
use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, Layer, LayerShell};
use relm4::{factory::FactoryVecDeque, prelude::*};

use crate::notifications::{
    card::{NotificationCard, NotificationCardOutput},
    types::{Notification, NotificationUrgency},
};

#[derive(Debug)]
pub struct FreshNotifications {
    visible: bool,
    notifications: FactoryVecDeque<NotificationCard>,
    monitor: Monitor,
    auto_dismiss_timeouts: HashMap<u32, glib::SourceId>,
}

#[derive(Debug)]
pub enum FreshNotificationsMsg {
    NewNotification(Notification),
    RemoveNotification(u32),
    AutoDismiss(u32),                // auto-dismiss a notification by ID
    NotificationAction(u32, String), // notification_id, action_id
    DismissNotification(u32),        // notification_id
}

#[derive(Debug)]
pub enum FreshNotificationsOutput {
    NotificationDismissed(u32),
    NotificationActionTriggered(u32, String),
}

#[relm4::component(pub)]
impl SimpleComponent for FreshNotifications {
    type Init = Monitor;
    type Input = FreshNotificationsMsg;
    type Output = FreshNotificationsOutput;

    view! {
        #[root]
        window = gtk4::Window {
            #[watch]
            set_visible: model.visible && !model.notifications.is_empty(),

            #[local_ref]
            notifications_container -> gtk4::Box {
                set_orientation: gtk4::Orientation::Vertical,
                set_spacing: 8,
                set_width_request: 400,
            },
        }
    }

    fn init(
        monitor: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let notifications = FactoryVecDeque::builder()
            .launch(gtk4::Box::default())
            .forward(sender.input_sender(), |output| match output {
                NotificationCardOutput::Dismiss(id) => {
                    FreshNotificationsMsg::DismissNotification(id)
                }
                NotificationCardOutput::Action(id, action) => {
                    FreshNotificationsMsg::NotificationAction(id, action)
                }
            });

        let model = FreshNotifications {
            visible: true,
            notifications,
            monitor,
            auto_dismiss_timeouts: HashMap::new(),
        };

        let notifications_container = model.notifications.widget();
        let widgets = view_output!();

        // configure layer shell after window creation
        widgets.window.init_layer_shell();
        widgets.window.set_layer(Layer::Overlay);
        widgets.window.set_exclusive_zone(-1); // don't reserve space
        widgets.window.set_anchor(Edge::Top, true);
        widgets.window.set_anchor(Edge::Right, true);
        widgets.window.set_monitor(Some(&model.monitor));
        widgets.window.set_margin(Edge::Top, 32);

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            FreshNotificationsMsg::NewNotification(notification) => {
                // add to the beginning (top) of the list
                let notification_id = notification.id;
                let urgency = notification.urgency;

                let mut guard = self.notifications.guard();
                guard.push_front(notification);
                drop(guard);

                // set up auto-dismiss for non-critical notifications
                if !matches!(urgency, NotificationUrgency::Critical) {
                    let dismiss_sender = sender.clone();
                    let timeout_id = glib::timeout_add_local_once(
                        std::time::Duration::from_secs(10),
                        move || {
                            dismiss_sender
                                .input(FreshNotificationsMsg::AutoDismiss(notification_id));
                        },
                    );
                    self.auto_dismiss_timeouts
                        .insert(notification_id, timeout_id);
                }
            }
            FreshNotificationsMsg::RemoveNotification(id) => {
                // remove auto-dismiss timeout if it exists
                if let Some(timeout_id) = self.auto_dismiss_timeouts.remove(&id) {
                    timeout_id.remove();
                }

                // remove from notifications list
                let mut guard = self.notifications.guard();
                let mut index_to_remove = None;

                for (index, item) in guard.iter().enumerate() {
                    if item.notification_id() == id {
                        index_to_remove = Some(index);
                        break;
                    }
                }

                if let Some(index) = index_to_remove {
                    guard.remove(index);
                }
            }
            FreshNotificationsMsg::AutoDismiss(id) => {
                // remove the timeout tracking since it fired
                self.auto_dismiss_timeouts.remove(&id);

                // remove the notification and notify service
                sender.input(FreshNotificationsMsg::RemoveNotification(id));
                sender.input(FreshNotificationsMsg::DismissNotification(id));
            }
            FreshNotificationsMsg::DismissNotification(id) => {
                // remove from our display
                sender.input(FreshNotificationsMsg::RemoveNotification(id));

                // forward to output
                sender
                    .output(FreshNotificationsOutput::NotificationDismissed(id))
                    .unwrap_or_else(|_| {
                        log::error!("couldn't output action trigger event from popup")
                    });
            }
            FreshNotificationsMsg::NotificationAction(id, action) => {
                sender
                    .output(FreshNotificationsOutput::NotificationActionTriggered(
                        id, action,
                    ))
                    .unwrap_or_else(|_| {
                        log::error!("couldn't output action trigger event from popup")
                    });
            }
        }
    }
}
