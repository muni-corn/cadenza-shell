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
        types::{InvokedAction, Notification},
    },
    settings,
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
    DismissAll,
    // payload is unused; update_view reads directly from the global
    StateUpdate,
    DismissNotification(u32),
    NotificationAction(InvokedAction),
}

#[derive(Debug)]
pub struct ActionPanelWidgets {
    window: gtk4::Window,
    cards: FactoryVecDeque<NotificationCard>,
    panel: gtk4::Box,
    clock: Controller<AnalogClock>,
    time_label: gtk4::Label,
    date_label: gtk4::Label,
    clear_all_button: gtk4::Button,
    clock_timer: Option<glib::SourceId>,
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
        // subscribe to the global notifications state; payload ignored —
        // update_view reads directly from the global on each notification
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
        window.set_width_request(settings::get_config().notifications.center_width);

        let mut widgets = ActionPanelWidgets {
            window,
            cards: FactoryVecDeque::builder()
                .launch(gtk4::Box::default())
                .forward(sender.input_sender(), |output| match output {
                    NotificationCardOutput::Dismiss(id) => ActionPanelMsg::DismissNotification(id),
                    NotificationCardOutput::Action(action) => {
                        ActionPanelMsg::NotificationAction(action)
                    }
                }),
            panel: gtk4::Box::builder()
                .css_classes(["notification-center", "background"])
                .orientation(gtk4::Orientation::Vertical)
                .hexpand(true)
                .vexpand(true)
                .visible(true)
                .build(),
            clock: AnalogClock::builder()
                .launch(settings::get_config().tiles.analog_clock_radius)
                .detach(),
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
            clock_timer: None,
            clear_all_button: gtk4::Button::builder()
                .label("Clear all")
                .halign(gtk4::Align::End)
                .visible(false)
                .build(),
        };
        widgets.clear_all_button.connect_clicked({
            let sender = sender.clone();
            move |_| sender.input(ActionPanelMsg::DismissAll)
        });

        // horizontal row holding both the analog clock and the digital
        // clock/date
        let clock_row = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(16)
            .margin_bottom(12)
            .build();

        // vertical stack for the digital time and date, anchored to the start
        // (top)
        let clock_text_box = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Vertical)
            .valign(gtk4::Align::Center)
            .build();

        clock_text_box.append(&widgets.time_label);
        clock_text_box.append(&widgets.date_label);

        clock_row.append(widgets.clock.widget());
        clock_row.append(&clock_text_box);

        widgets.panel.append(&clock_row);
        widgets.panel.append(&widgets.clear_all_button);
        widgets.panel.append(widgets.cards.widget());
        widgets.window.set_child(Some(&widgets.panel));

        // update the digital clock and date labels every second; weak refs
        // keep the timer from holding the labels alive
        let time_label = widgets.time_label.downgrade();
        let date_label = widgets.date_label.downgrade();
        let clock_timer = glib::timeout_add_local(std::time::Duration::from_secs(1), move || {
            let (Some(time_label), Some(date_label)) = (time_label.upgrade(), date_label.upgrade())
            else {
                return ControlFlow::Break;
            };
            time_label.set_label(&format_time());
            date_label.set_label(&format_date());
            ControlFlow::Continue
        });

        widgets.clock_timer = Some(clock_timer);

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            ActionPanelMsg::Toggle => {
                let next = !self.visible;
                tracing::debug!(visible = next, "notification center toggled");
                self.visible = next;
            }
            ActionPanelMsg::DismissAll => {
                tracing::debug!("notification center dismiss-all triggered");
                crate::notifications::clear_all();
            }
            ActionPanelMsg::StateUpdate => {
                // view is rebuilt from the global in update_view
            }
            ActionPanelMsg::DismissNotification(id) => {
                tracing::trace!(notification.id = id, "notification dismissed from center");
                crate::notifications::dismiss(id);
            }
            ActionPanelMsg::NotificationAction(action) => {
                tracing::trace!(
                    notification.id = action.id,
                    action = %action.key,
                    "notification action invoked from center"
                );
                crate::notifications::invoke_action(action);
            }
        }
    }

    fn update_view(&self, widgets: &mut Self::Widgets, _sender: ComponentSender<Self>) {
        widgets.window.set_visible(self.visible);

        if self.visible {
            let state = NOTIFICATIONS_STATE.read();
            widgets
                .clear_all_button
                .set_visible(!state.notifications.is_empty());
            let mut notifications: Vec<&Notification> = state.notifications.values().collect();
            // timestamps are whole seconds, so ties are broken by id (newer
            // first)
            notifications.sort_by_key(|n| Reverse((n.timestamp, n.id)));

            let mut guard = widgets.cards.guard();
            guard.clear();
            for notification in notifications {
                guard.push_back(notification.clone());
            }
        }
    }

    fn shutdown(&mut self, widgets: &mut Self::Widgets, _output: relm4::Sender<Self::Output>) {
        // dropping the controller doesn't destroy a toplevel, so the panel
        // would otherwise outlive its bar and jump to another output
        widgets.window.destroy();

        // the labels are still alive here, so the timer hasn't broken out
        // on its own and removing it can't fail
        if let Some(clock_timer) = widgets.clock_timer.take() {
            clock_timer.remove();
        }
    }
}
