use std::cmp::Reverse;

use gdk4::Monitor;
use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, Layer, LayerShell};
use relm4::{factory::FactoryVecDeque, prelude::*};

use crate::{
    analog_clock::AnalogClock,
    notifications::{
        NOTIFICATIONS_STATE,
        card::{NotificationCard, NotificationCardOutput},
        types::Notification,
    },
};

#[derive(Debug)]
pub struct ActionPanel {
    // stored to keep the monitor object alive for the layer-shell window
    monitor: Monitor,
    visible: bool,
}

#[derive(Debug)]
pub enum ActionPanelMsg {
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
pub struct ActionPanelWidgets {
    window: gtk4::Window,
    cards: FactoryVecDeque<NotificationCard>,
    panel: gtk4::Box,
    clock: Controller<AnalogClock>,
}

impl SimpleComponent for ActionPanel {
    type Init = Monitor;
    type Input = ActionPanelMsg;
    type Output = ();
    type Root = gtk4::Window;
    type Widgets = ActionPanelWidgets;

    fn init_root() -> Self::Root {
        gtk4::Window::builder()
            .title("cadenza action panel")
            .visible(false)
            .build()
    }

    fn init(
        monitor: Self::Init,
        window: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        // subscribe to the global notifications state; payload ignored — update_view
        // reads directly from the global on each notification
        NOTIFICATIONS_STATE.subscribe(sender.input_sender(), |_| ActionPanelMsg::StateUpdate);

        let model = ActionPanel {
            monitor,
            visible: false,
        };

        let cards = FactoryVecDeque::builder()
            .launch(gtk4::Box::default())
            .forward(sender.input_sender(), |output| match output {
                NotificationCardOutput::Dismiss(id) => ActionPanelMsg::DismissNotification(id),
                NotificationCardOutput::Action(id, action) => {
                    ActionPanelMsg::NotificationAction(id, action)
                }
            });

        let panel = gtk4::Box::builder()
            .css_classes(["notification-center"])
            .hexpand(true)
            .vexpand(true)
            .visible(true)
            .build();

        // set up layer shell properties
        window.init_layer_shell();
        window.set_monitor(Some(&model.monitor));
        window.set_namespace(Some("notification-center"));
        window.set_layer(Layer::Top);
        window.set_anchor(Edge::Top, true);
        window.set_anchor(Edge::Right, true);
        window.set_anchor(Edge::Bottom, true);
        window.set_margin_all(8);
        window.set_width_request(432);

        let widgets = ActionPanelWidgets {
            window,
            cards,
            panel,
            clock: AnalogClock::builder().launch(32.0).detach(),
        };

        widgets.panel.append(widgets.clock.widget());
        widgets.panel.append(widgets.cards.widget());
        widgets.window.set_child(Some(&widgets.panel));

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            ActionPanelMsg::Toggle => {
                self.visible = !self.visible;
            }
            ActionPanelMsg::DismissAll => {
                crate::notifications::clear_all();
            }
            ActionPanelMsg::StateUpdate => {
                // view is rebuilt from the global in update_view
            }
            ActionPanelMsg::DismissNotification(id) => {
                crate::notifications::dismiss(id);
            }
            ActionPanelMsg::NotificationAction(id, action) => {
                crate::notifications::invoke_action(id, action);
            }
        }
    }

    fn update_view(&self, widgets: &mut Self::Widgets, _sender: ComponentSender<Self>) {
        widgets.window.set_visible(self.visible);

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
