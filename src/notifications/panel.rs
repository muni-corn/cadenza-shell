use std::cmp::Reverse;

use chrono::Local;
use gdk4::Monitor;
use glib::ControlFlow;
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
    time_label: gtk4::Label,
    date_label: gtk4::Label,
}

/// Formats the current time as a 12-hour clock string (e.g. "2:34 PM").
fn format_time() -> String {
    Local::now().format("%-I:%M %P").to_string()
}

/// Formats the current date as a full readable string (e.g. "Sunday, May 10,
/// 2026").
fn format_date() -> String {
    Local::now().format("%A, %B %-d, %Y").to_string()
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
            cards: FactoryVecDeque::builder()
                .launch(gtk4::Box::default())
                .forward(sender.input_sender(), |output| match output {
                    NotificationCardOutput::Dismiss(id) => ActionPanelMsg::DismissNotification(id),
                    NotificationCardOutput::Action(id, action) => {
                        ActionPanelMsg::NotificationAction(id, action)
                    }
                }),
            panel: gtk4::Box::builder()
                .css_classes(["notification-center"])
                .orientation(gtk4::Orientation::Vertical)
                .hexpand(true)
                .vexpand(true)
                .visible(true)
                .build(),
            clock: AnalogClock::builder().launch(32.0).detach(),
            time_label: gtk4::Label::builder()
                .label(format_time())
                .css_classes(["big-clock"])
                .halign(gtk4::Align::Start)
                .build(),
            date_label: gtk4::Label::builder()
                .label(format_date())
                .css_classes(["date-label"])
                .halign(gtk4::Align::Start)
                .build(),
        };

        // horizontal row holding both the analog clock and the digital clock/date
        let clock_row = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(16)
            .margin_bottom(12)
            .build();

        // vertical stack for the digital time and date, anchored to the start (top)
        let clock_text_box = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .valign(gtk4::Align::Center)
            .build();

        clock_text_box.append(&widgets.time_label);
        clock_text_box.append(&widgets.date_label);

        clock_row.append(widgets.clock.widget());
        clock_row.append(&clock_text_box);

        widgets.panel.append(&clock_row);
        widgets.panel.append(widgets.cards.widget());
        widgets.window.set_child(Some(&widgets.panel));

        // update the digital clock and date labels every second
        let time_label_clone = widgets.time_label.clone();
        let date_label_clone = widgets.date_label.clone();
        glib::timeout_add_local(std::time::Duration::from_secs(1), move || {
            time_label_clone.set_label(&format_time());
            date_label_clone.set_label(&format_date());
            ControlFlow::Continue
        });

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
