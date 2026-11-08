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
    cards: FactoryVecDeque<NotificationCard>,
    monitor: Monitor,
    /// Generation of the pending auto-dismiss timer for each popup.
    auto_dismiss_timeouts: HashMap<u32, u64>,
    next_timer_generation: u64,
}

#[derive(Debug)]
pub enum FreshNotificationsMsg {
    NewNotification(Notification),
    RemoveNotification(u32),
    NotificationAction(u32, String), // notification_id, action_id
    DismissNotification(u32),        // notification_id
}

#[derive(Debug)]
pub enum FreshNotificationsOutput {
    NotificationDismissed(u32),
    NotificationActionTriggered(u32, String),
}

#[relm4::component(pub)]
impl Component for FreshNotifications {
    /// Notification id and timer generation of an elapsed auto-dismiss timer.
    type CommandOutput = (u32, u64);
    type Init = Monitor;
    type Input = FreshNotificationsMsg;
    type Output = FreshNotificationsOutput;

    view! {
        #[root]
        window = gtk4::Window {
            #[watch]
            set_visible: model.visible && !model.cards.is_empty(),

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
        let cards = FactoryVecDeque::builder()
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
            cards,
            monitor,
            auto_dismiss_timeouts: HashMap::new(),
            next_timer_generation: 0,
        };

        let notifications_container = model.cards.widget();
        let widgets = view_output!();

        // configure layer shell after window creation
        widgets.window.init_layer_shell();
        widgets.window.set_layer(Layer::Overlay);
        widgets.window.set_namespace(Some("notifications"));
        widgets.window.set_exclusive_zone(-1); // don't reserve space
        widgets.window.set_anchor(Edge::Top, true);
        widgets.window.set_anchor(Edge::Right, true);
        widgets.window.set_monitor(Some(&model.monitor));
        widgets.window.set_margin(Edge::Top, 32);

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>, _root: &Self::Root) {
        match msg {
            FreshNotificationsMsg::NewNotification(notification) => {
                let notification_id = notification.id;
                let urgency = notification.urgency;

                tracing::debug!(
                    notification.id = notification_id,
                    ?urgency,
                    "queued fresh popup"
                );

                // add to the beginning (top) of the list
                self.cards.guard().push_front(notification);

                // set up auto-dismiss for non-critical notifications
                if !matches!(urgency, NotificationUrgency::Critical) {
                    let generation = self.next_timer_generation;
                    self.next_timer_generation = generation.wrapping_add(1);
                    self.auto_dismiss_timeouts
                        .insert(notification_id, generation);

                    // relm4 cancels the command if the panel shuts down first
                    sender.oneshot_command(async move {
                        tokio::time::sleep(AUTO_DISMISS_DELAY).await;
                        (notification_id, generation)
                    });
                } else {
                    tracing::debug!(
                        notification.id = notification_id,
                        "critical notification; skipping auto-dismiss"
                    );
                }
            }
            FreshNotificationsMsg::RemoveNotification(id) => {
                // forget the pending timer so it's ignored when it elapses
                if self.auto_dismiss_timeouts.remove(&id).is_some() {
                    tracing::debug!(notification.id = id, "cancelled auto-dismiss timeout");
                }

                // remove from notifications list
                let mut guard = self.cards.guard();
                let index_to_remove = guard
                    .iter()
                    .enumerate()
                    .find(|(_, item)| item.notification_id() == id)
                    .map(|(index, _)| index);

                match index_to_remove {
                    Some(index) => {
                        tracing::debug!(notification.id = id, index, "removing fresh popup card");
                        guard.remove(index);
                    }
                    None => {
                        // this can happen when a removal races a previous
                        // removal or when a notification was never shown as a
                        // popup
                        tracing::warn!(
                            notification.id = id,
                            "removal requested for a notification not in fresh cards (state \
                             desync?)"
                        );
                    }
                }
            }
            FreshNotificationsMsg::DismissNotification(id) => {
                tracing::debug!(
                    notification.id = id,
                    "dismiss notification forwarded to output"
                );

                // remove from our display
                sender.input(FreshNotificationsMsg::RemoveNotification(id));

                // forward to output
                sender
                    .output(FreshNotificationsOutput::NotificationDismissed(id))
                    .unwrap_or_else(|_| {
                        tracing::error!(
                            notification.id = id,
                            "couldn't output dismiss event from popup"
                        )
                    });
            }
            FreshNotificationsMsg::NotificationAction(id, action) => {
                tracing::debug!(notification.id = id, %action, "notification action triggered from popup");

                sender
                    .output(FreshNotificationsOutput::NotificationActionTriggered(
                        id, action,
                    ))
                    .unwrap_or_else(|_| {
                        tracing::error!(
                            notification.id = id,
                            "couldn't output action trigger event from popup"
                        )
                    });
            }
        }
    }

    fn shutdown(&mut self, widgets: &mut Self::Widgets, _output: relm4::Sender<Self::Output>) {
        // dropping the controller doesn't destroy a toplevel, so popups would
        // otherwise outlive their monitor and jump to another output
        widgets.window.destroy();
    }

    fn update_cmd(
        &mut self,
        (id, generation): Self::CommandOutput,
        sender: ComponentSender<Self>,
        _root: &Self::Root,
    ) {
        // the popup was removed or replaced since this timer started
        if self.auto_dismiss_timeouts.get(&id) != Some(&generation) {
            return;
        }

        tracing::debug!(notification.id = id, "auto-dismiss timer fired");
        self.auto_dismiss_timeouts.remove(&id);

        // remove the notification and notify service
        sender.input(FreshNotificationsMsg::RemoveNotification(id));
        sender.input(FreshNotificationsMsg::DismissNotification(id));
    }
}

/// How long a non-critical popup stays on screen.
const AUTO_DISMISS_DELAY: std::time::Duration = std::time::Duration::from_secs(10);
