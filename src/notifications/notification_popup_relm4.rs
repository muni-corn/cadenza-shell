use gdk4::Monitor;
use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, Layer, LayerShell};
use relm4::factory::FactoryVecDeque;
use relm4::prelude::*;
use std::collections::HashMap;

use super::notification_card_relm4::{NotificationCard, NotificationCardOutput, NotificationData};
use crate::services::notifications::NotificationService;

#[derive(Debug)]
pub struct NotificationPopup {
    visible: bool,
    notifications: FactoryVecDeque<NotificationCard>,
    monitor: Monitor,
    service: NotificationService,
    auto_dismiss_timeouts: HashMap<u32, glib::SourceId>,
}

#[derive(Debug)]
pub enum NotificationPopupMsg {
    Toggle,
    SetVisible(bool),
    AddNotification(NotificationData),
    RemoveNotification(u32),
    UpdateNotifications(Vec<NotificationData>),
    AutoDismiss(u32),                // Auto-dismiss a notification by ID
    NotificationAction(u32, String), // notification_id, action_id
    DismissNotification(u32),        // notification_id
    CheckService,                    // Periodic service check
}

#[derive(Debug)]
pub enum NotificationPopupOutput {
    NotificationDismissed(u32),
    NotificationActionTriggered(u32, String),
    DefaultActionTriggered(u32),
}

#[relm4::component(pub)]
impl SimpleComponent for NotificationPopup {
    type Init = (Monitor, NotificationService);
    type Input = NotificationPopupMsg;
    type Output = NotificationPopupOutput;

    view! {
        #[root]
        window = gtk4::ApplicationWindow {
            set_title: Some("Muse Shell Notification Popup"),
            #[watch]
            set_visible: model.visible && !model.notifications.is_empty(),

            // Layer shell setup is done in init

            #[local_ref]
            notifications_container -> gtk4::Box {
                set_orientation: gtk4::Orientation::Vertical,
                set_spacing: 8,
                set_width_request: 400,
            },
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let (monitor, service) = init;

        let notifications = FactoryVecDeque::builder()
            .launch(gtk4::Box::default())
            .forward(sender.input_sender(), |output| match output {
                NotificationCardOutput::Dismiss(id) => {
                    NotificationPopupMsg::DismissNotification(id)
                }
                NotificationCardOutput::Action(id, action) => {
                    NotificationPopupMsg::NotificationAction(id, action)
                }
                NotificationCardOutput::DefaultAction(id) => {
                    // Convert to action output immediately
                    NotificationPopupMsg::NotificationAction(id, "default".to_string())
                }
            });

        let model = NotificationPopup {
            visible: true,
            notifications,
            monitor: monitor.clone(),
            service: service.clone(),
            auto_dismiss_timeouts: HashMap::new(),
        };

        let notifications_container = model.notifications.widget();
        let widgets = view_output!();

        // Configure layer shell after window creation
        widgets.window.init_layer_shell();
        widgets.window.set_layer(Layer::Overlay);
        widgets.window.set_exclusive_zone(-1); // Don't reserve space
        widgets.window.set_anchor(Edge::Top, true);
        widgets.window.set_anchor(Edge::Right, true);
        widgets.window.set_monitor(Some(&monitor));
        widgets.window.set_margin(Edge::Top, 8);
        widgets.window.set_margin(Edge::Right, 8);

        // Set up periodic service checking
        let service_sender = sender.clone();
        glib::timeout_add_local(std::time::Duration::from_millis(500), move || {
            service_sender.input(NotificationPopupMsg::CheckService);
            glib::ControlFlow::Continue
        });

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            NotificationPopupMsg::Toggle => {
                self.visible = !self.visible;
            }
            NotificationPopupMsg::SetVisible(visible) => {
                self.visible = visible;
            }
            NotificationPopupMsg::AddNotification(notification) => {
                // Add to the beginning (top) of the list
                let notification_id = notification.id;
                let urgency = notification.urgency;

                let mut guard = self.notifications.guard();
                guard.push_front(notification);
                drop(guard);

                // Set up auto-dismiss for non-critical notifications
                if urgency < 2 {
                    let dismiss_sender = sender.clone();
                    let timeout_id = glib::timeout_add_local_once(
                        std::time::Duration::from_secs(10),
                        move || {
                            dismiss_sender
                                .input(NotificationPopupMsg::AutoDismiss(notification_id));
                        },
                    );
                    self.auto_dismiss_timeouts
                        .insert(notification_id, timeout_id);
                }
            }
            NotificationPopupMsg::RemoveNotification(id) => {
                // Remove auto-dismiss timeout if it exists
                if let Some(timeout_id) = self.auto_dismiss_timeouts.remove(&id) {
                    timeout_id.remove();
                }

                // Remove from notifications list
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
            NotificationPopupMsg::UpdateNotifications(notifications) => {
                // Clear existing timeouts
                for timeout_id in self.auto_dismiss_timeouts.drain() {
                    timeout_id.1.remove();
                }
                self.auto_dismiss_timeouts.clear();

                // Update notifications list
                let mut guard = self.notifications.guard();
                guard.clear();

                // Sort notifications by timestamp (newest first)
                let mut sorted_notifications = notifications;
                sorted_notifications.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

                for notification in sorted_notifications {
                    let notification_id = notification.id;
                    let urgency = notification.urgency;

                    guard.push_back(notification);

                    // Set up auto-dismiss for non-critical notifications
                    if urgency < 2 {
                        let dismiss_sender = sender.clone();
                        let timeout_id = glib::timeout_add_local_once(
                            std::time::Duration::from_secs(10),
                            move || {
                                dismiss_sender
                                    .input(NotificationPopupMsg::AutoDismiss(notification_id));
                            },
                        );
                        self.auto_dismiss_timeouts
                            .insert(notification_id, timeout_id);
                    }
                }
            }
            NotificationPopupMsg::AutoDismiss(id) => {
                // Remove the timeout tracking since it fired
                self.auto_dismiss_timeouts.remove(&id);

                // Remove the notification and notify service
                sender.input(NotificationPopupMsg::RemoveNotification(id));
                sender.input(NotificationPopupMsg::DismissNotification(id));
            }
            NotificationPopupMsg::DismissNotification(id) => {
                // Remove from our display
                sender.input(NotificationPopupMsg::RemoveNotification(id));

                // Notify the service
                use gtk4::subclass::prelude::ObjectSubclassIsExt;
                self.service.imp().remove_notification(id);

                // Forward to output
                let _ = sender.output(NotificationPopupOutput::NotificationDismissed(id));
            }
            NotificationPopupMsg::NotificationAction(id, action) => {
                // Remove from our display since action was taken
                sender.input(NotificationPopupMsg::RemoveNotification(id));

                // Forward to output
                if action == "default" {
                    let _ = sender.output(NotificationPopupOutput::DefaultActionTriggered(id));
                } else {
                    let _ = sender.output(NotificationPopupOutput::NotificationActionTriggered(
                        id, action,
                    ));
                }
            }
            NotificationPopupMsg::CheckService => {
                // Check if we need to update our notifications list based on service state
                let current_notifications = self.service.get_notifications();
                let current_data: Vec<NotificationData> = current_notifications
                    .into_iter()
                    .map(|n| n.into())
                    .collect();

                // Only update if there's a difference in count or content
                if current_data.len() != self.notifications.len() {
                    sender.input(NotificationPopupMsg::UpdateNotifications(current_data));
                }
            }
        }
    }
}

pub fn create_notification_popup(
    monitor: Monitor,
    service: NotificationService,
) -> Controller<NotificationPopup> {
    NotificationPopup::builder()
        .launch((monitor, service))
        .detach()
}

// Export the component type for public use
pub type NotificationPopupComponent = NotificationPopup;
