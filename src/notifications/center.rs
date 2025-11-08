use std::collections::HashMap;

use gdk4::Monitor;
use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, LayerShell};
use relm4::{factory::FactoryVecDeque, prelude::*};

use crate::notifications::{
    NotificationService, NotificationServiceMsg, NotificationWorkerOutput,
    card::{NotificationCard, NotificationCardOutput},
    types::Notification,
};

#[derive(Debug)]
pub struct NotificationCenter {
    monitor: Monitor,
    notification_worker: relm4::WorkerController<NotificationService>,
    notifications: HashMap<u32, Notification>,
    visible: bool,
}

#[derive(Debug)]
pub enum NotificationCenterMsg {
    Toggle,
    DismissAll,
    ServiceUpdate(NotificationWorkerOutput),
    DismissNotification(u32),
    NotificationAction(u32, String),
}

#[derive(Debug)]
pub struct NotificationCenterWidgets {
    root: gtk4::Window,
    cards: FactoryVecDeque<NotificationCard>,
}

impl SimpleComponent for NotificationCenter {
    type Init = Monitor;
    type Input = NotificationCenterMsg;
    type Output = ();
    type Root = gtk4::Window;
    type Widgets = NotificationCenterWidgets;

    fn init_root() -> Self::Root {
        gtk4::Window::builder()
            .title("notification-center")
            .visible(false)
            .build()
    }

    fn init(
        monitor: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        // Initialize notification worker
        let notification_worker = NotificationService::builder()
            .detach_worker(())
            .forward(sender.input_sender(), NotificationCenterMsg::ServiceUpdate);

        let cards = FactoryVecDeque::builder()
            .launch(gtk4::Box::default())
            .forward(sender.input_sender(), |output| match output {
                NotificationCardOutput::Dismiss(id) => {
                    NotificationCenterMsg::DismissNotification(id)
                }
                NotificationCardOutput::Action(id, action) => {
                    NotificationCenterMsg::NotificationAction(id, action)
                }
            });

        // Set up layer shell properties
        root.init_layer_shell();
        root.set_monitor(Some(&monitor));
        root.set_namespace(Some("notification-center"));
        root.set_anchor(Edge::Top, true);
        root.set_anchor(Edge::Right, true);
        root.set_anchor(Edge::Bottom, true);
        root.set_margin_all(8);
        root.set_width_request(432);

        let model = NotificationCenter {
            monitor,
            notification_worker,
            notifications: HashMap::new(),
            visible: false,
        };

        // Request initial notifications
        model
            .notification_worker
            .emit(NotificationServiceMsg::GetNotifications);

        ComponentParts {
            model,
            widgets: NotificationCenterWidgets { root, cards },
        }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            NotificationCenterMsg::Toggle => {
                self.visible = !self.visible;

                // refresh notifications when opening
                if self.visible {
                    self.notification_worker
                        .emit(NotificationServiceMsg::GetNotifications);
                }
            }
            NotificationCenterMsg::DismissAll => {
                self.notification_worker
                    .emit(NotificationServiceMsg::ClearAll);
            }
            NotificationCenterMsg::ServiceUpdate(output) => {
                match output {
                    NotificationWorkerOutput::Notifications(notifications) => {
                        self.notifications = notifications;
                    }
                    NotificationWorkerOutput::NotificationReceived(notification) => {
                        self.notifications.insert(notification.id, notification);
                    }
                    NotificationWorkerOutput::NotificationClosed { id, .. } => {
                        self.notifications.remove(&id);
                    }
                    NotificationWorkerOutput::AllCleared => {
                        self.notifications.clear();
                    }
                    NotificationWorkerOutput::ActionInvoked { .. } => {
                        // Handle action invoked if needed
                    }
                    NotificationWorkerOutput::Error(e) => {
                        log::error!("notification service error: {}", e);
                    }
                }
            }
            NotificationCenterMsg::DismissNotification(id) => {
                self.notification_worker
                    .emit(NotificationServiceMsg::CloseNotification(id));
            }
            NotificationCenterMsg::NotificationAction(id, action) => {
                self.notification_worker
                    .emit(NotificationServiceMsg::ActionInvoked(id, action));
            }
        }

        log::debug!("notification center updated: {:?}", self);
    }

    fn update_view(&self, widgets: &mut Self::Widgets, _sender: ComponentSender<Self>) {
        // Update window visibility
        widgets.root.set_visible(self.visible);

        // Update the FactoryVecDeque with current notifications
        if self.visible {
            // Sort notifications by timestamp (newest first)
            let mut notifications: Vec<&Notification> = self.notifications.values().collect();
            notifications.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

            // Clear and repopulate the FactoryVecDeque
            {
                let mut guard = widgets.cards.guard();
                guard.clear();
                for notification in notifications {
                    guard.push_back(notification.clone());
                }
            }
        }
    }
}
