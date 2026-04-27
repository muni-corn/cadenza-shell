use std::cmp::Reverse;

use gdk4::Monitor;
use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, LayerShell};
use relm4::{factory::FactoryVecDeque, prelude::*};

use crate::notifications::{
    NOTIFICATIONS_STATE,
    card::{NotificationCard, NotificationCardOutput},
    types::Notification,
};

#[derive(Debug)]
pub struct NotificationCenter {
    // stored to keep the monitor object alive for the layer-shell window
    _monitor: Monitor,
    visible: bool,
}

#[derive(Debug)]
pub enum NotificationCenterMsg {
    Toggle,
    // wired to a future "clear all" button in the notification center ui
    #[allow(dead_code)]
    DismissAll,
    // payload is unused; update_view reads directly from the global
    StateUpdate,
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
        // subscribe to the global notifications state; payload ignored — update_view
        // reads directly from the global on each notification
        NOTIFICATIONS_STATE.subscribe(sender.input_sender(), |_| {
            NotificationCenterMsg::StateUpdate
        });

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

        // set up layer shell properties
        root.init_layer_shell();
        root.set_monitor(Some(&monitor));
        root.set_namespace(Some("notification-center"));
        root.set_anchor(Edge::Top, true);
        root.set_anchor(Edge::Right, true);
        root.set_anchor(Edge::Bottom, true);
        root.set_margin_all(8);
        root.set_width_request(432);

        let model = NotificationCenter {
            _monitor: monitor,
            visible: false,
        };

        ComponentParts {
            model,
            widgets: NotificationCenterWidgets { root, cards },
        }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            NotificationCenterMsg::Toggle => {
                self.visible = !self.visible;
            }
            NotificationCenterMsg::DismissAll => {
                crate::notifications::clear_all();
            }
            NotificationCenterMsg::StateUpdate => {
                // view is rebuilt from the global in update_view
            }
            NotificationCenterMsg::DismissNotification(id) => {
                crate::notifications::dismiss(id);
            }
            NotificationCenterMsg::NotificationAction(id, action) => {
                crate::notifications::invoke_action(id, action);
            }
        }
    }

    fn update_view(&self, widgets: &mut Self::Widgets, _sender: ComponentSender<Self>) {
        widgets.root.set_visible(self.visible);

        if self.visible {
            let state = NOTIFICATIONS_STATE.read();
            let mut notifications: Vec<&Notification> = state.notifications.values().collect();
            notifications.sort_by_key(|n| Reverse(n.timestamp));

            let mut guard = widgets.cards.guard();
            guard.clear();
            for notification in notifications {
                guard.push_back(notification.clone());
            }
        }
    }
}
