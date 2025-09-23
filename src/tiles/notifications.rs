use std::collections::HashMap;

use gdk4::Monitor;
use gtk4::prelude::*;
use relm4::{WorkerController, prelude::*};

use crate::{
    notifications::fresh_notifications::{
        FreshNotifications, FreshNotificationsMsg, FreshNotificationsOutput as FreshNotificationsOutput,
    },
    services::notifications::{
        Notification, NotificationService, NotificationServiceMsg, NotificationWorkerOutput,
    },
    tiles::Attention,
    widgets::tile::{Tile, TileMsg, TileOutput},
};

const NOTIFICATION_ICON: &str = "󰂚";
const NOTIFICATION_NEW_ICON: &str = "󰂛";

#[derive(Debug)]
pub struct NotificationsTile {
    notification_worker: WorkerController<NotificationService>,
    notification_count: u32,
    tile: Controller<Tile>,
    popups: HashMap<String, Controller<FreshNotifications>>, // monitor_name -> popup
    active_notifications: HashMap<u32, Notification>,
}

#[derive(Debug)]
pub enum NotificationsTileMsg {
    TileClicked,
    ServiceUpdate(NotificationWorkerOutput),
    TogglePopup,
    PopupOutput(FreshNotificationsOutput),
    MonitorAdded(Monitor),
    Nothing,
}

pub struct NotificationsTileWidgets {
    _root: <NotificationsTile as Component>::Root,
}

impl SimpleComponent for NotificationsTile {
    type Init = ();
    type Input = NotificationsTileMsg;
    type Output = ();
    type Root = gtk::Box;
    type Widgets = NotificationsTileWidgets;

    fn init(
        _init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        // initialize notification worker
        let notification_worker = NotificationService::builder()
            .detach_worker(())
            .forward(sender.input_sender(), NotificationsTileMsg::ServiceUpdate);

        // initialize the tile component
        let tile =
            Tile::builder()
                .launch(Default::default())
                .forward(sender.input_sender(), |msg| match msg {
                    TileOutput::Clicked => NotificationsTileMsg::TileClicked,
                    _ => NotificationsTileMsg::Nothing,
                });

        // create popups for all existing monitors
        let display = gdk4::Display::default().expect("could not get default display");
        let monitors = display.monitors();

        for monitor in monitors.iter::<gdk4::Monitor>().flatten() {
            sender.input(NotificationsTileMsg::MonitorAdded(monitor));
        }

        root.append(tile.widget());

        let model = NotificationsTile {
            notification_worker,
            notification_count: 0,
            tile,
            popups: HashMap::new(),
            active_notifications: HashMap::new(),
        };

        ComponentParts {
            model,
            widgets: NotificationsTileWidgets { _root: root },
        }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            NotificationsTileMsg::TileClicked => {
                log::debug!("notifications tile clicked");
                // request current notifications
                self.notification_worker
                    .emit(NotificationServiceMsg::GetNotifications);
            }
            NotificationsTileMsg::ServiceUpdate(output) => {
                match output {
                    NotificationWorkerOutput::Notifications(notifications) => {
                        let count = notifications.len() as u32;
                        self.active_notifications = notifications;

                        if count != self.notification_count {
                            self.notification_count = count;

                            // update tile appearance based on notification count
                            let icon = if count > 0 {
                                NOTIFICATION_NEW_ICON
                            } else {
                                NOTIFICATION_ICON
                            };
                            let primary_text = if count > 0 {
                                Some(count.to_string())
                            } else {
                                None
                            };
                            let attention = if count > 0 {
                                Attention::Warning
                            } else {
                                Attention::Normal
                            };

                            self.tile.emit(TileMsg::SetIcon(Some(icon.to_string())));
                            self.tile.emit(TileMsg::SetPrimary(primary_text));
                            self.tile.emit(TileMsg::SetAttention(attention));
                        }
                    }
                    NotificationWorkerOutput::NotificationReceived(notification) => {
                        log::debug!("new notification received: {}", notification.id);

                        // store the notification
                        self.active_notifications
                            .insert(notification.id, notification.clone());

                        // show in all existing popups
                        for popup in self.popups.values() {
                            popup
                                .emit(FreshNotificationsMsg::NewNotification(notification.clone()));
                        }

                        // refresh notifications count
                        self.notification_worker
                            .emit(NotificationServiceMsg::GetNotifications);
                    }
                    NotificationWorkerOutput::NotificationClosed { id, reason: _ } => {
                        log::debug!("notification {} closed", id);

                        // remove from active notifications
                        self.active_notifications.remove(&id);

                        // remove from all popups
                        for popup in self.popups.values() {
                            popup.emit(FreshNotificationsMsg::RemoveNotification(id));
                        }

                        // refresh notifications count
                        self.notification_worker
                            .emit(NotificationServiceMsg::GetNotifications);
                    }
                    NotificationWorkerOutput::Error(e) => {
                        log::error!("notification worker error: {}", e);
                    }
                    _ => {}
                }
            }
            NotificationsTileMsg::Nothing => (),
            NotificationsTileMsg::PopupOutput(output) => {
                match output {
                    FreshNotificationsOutput::NotificationDismissed(id) => {
                        log::debug!("notification {} dismissed from popup", id);
                        // Tell the service to close this notification
                        self.notification_worker
                            .emit(NotificationServiceMsg::CloseNotification(id));
                    }
                    _ => {}
                }
            }
            NotificationsTileMsg::MonitorAdded(monitor) => {
                // create popup for this monitor
                if let Some(connector) = monitor.connector() {
                    let connector_str = connector.to_string();

                    if !self.popups.contains_key(&connector_str) {
                        log::debug!("creating notification popup for monitor: {}", connector_str);

                        let popup = FreshNotifications::builder()
                            .launch(monitor)
                            .forward(sender.input_sender(), NotificationsTileMsg::PopupOutput);

                        // Send existing notifications to the new popup
                        for notification in self.active_notifications.values() {
                            popup
                                .emit(FreshNotificationsMsg::NewNotification(notification.clone()));
                        }

                        self.popups.insert(connector_str, popup);
                    }
                }
            }
        }
    }

    fn init_root() -> Self::Root {
        gtk::Box::new(gtk::Orientation::Horizontal, 0)
    }
}
