use std::collections::HashMap;

use gtk4::prelude::*;
use relm4::{WorkerController, prelude::*};

use crate::{
    icon_names::{ALERT_BADGE_REGULAR, ALERT_REGULAR},
    notifications::{
        NotificationService, NotificationServiceMsg, NotificationWorkerOutput,
        fresh::{FreshNotifications, FreshNotificationsMsg, FreshNotificationsOutput},
        types::Notification,
    },
    tiles::Attention,
    widgets::tile::{Tile, TileMsg, TileOutput},
};

#[derive(Debug)]
pub struct NotificationsTile {
    notification_worker: WorkerController<NotificationService>,
    notification_count: u32,
    active_notifications: HashMap<u32, Notification>,
    fresh_panel: Controller<FreshNotifications>,
}

#[derive(Debug)]
pub enum NotificationsTileMsg {
    TileClicked,
    ServiceUpdate(NotificationWorkerOutput),
    NotificationDismissed(u32),
    ActionTriggered(u32, String),
    Nothing,
}

#[derive(Debug)]
pub enum NotificationsTileOutput {
    ToggleNotificationCenter,
}

#[derive(Debug)]
pub struct NotificationsTileWidgets {
    root: <NotificationsTile as Component>::Root,
    tile: Controller<Tile>,
}

impl SimpleComponent for NotificationsTile {
    type Init = ();
    type Input = NotificationsTileMsg;
    type Output = NotificationsTileOutput;
    type Root = gtk::Box;
    type Widgets = NotificationsTileWidgets;

    fn init(
        _init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        // create popups for first monitor only
        let display = gdk4::Display::default().expect("could not get default display");
        let monitor = display
            .monitors()
            .iter()
            .next()
            .expect("no monitor available for notifications")
            .expect("couldn't get available monitor for notifications");

        let widgets = NotificationsTileWidgets {
            root,
            tile: Tile::builder().launch(Default::default()).forward(
                sender.input_sender(),
                |msg| match msg {
                    TileOutput::Clicked => NotificationsTileMsg::TileClicked,
                    _ => NotificationsTileMsg::Nothing,
                },
            ),
        };

        let model = NotificationsTile {
            // initialize notification worker
            notification_worker: NotificationService::builder()
                .detach_worker(())
                .forward(sender.input_sender(), NotificationsTileMsg::ServiceUpdate),
            notification_count: 0,
            active_notifications: HashMap::new(),
            fresh_panel: FreshNotifications::builder().launch(monitor).forward(
                sender.input_sender(),
                |msg| match msg {
                    FreshNotificationsOutput::NotificationDismissed(id) => {
                        NotificationsTileMsg::NotificationDismissed(id)
                    }
                    FreshNotificationsOutput::NotificationActionTriggered(id, action) => {
                        NotificationsTileMsg::ActionTriggered(id, action)
                    }
                },
            ),
        };

        widgets.root.append(widgets.tile.widget());

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            NotificationsTileMsg::TileClicked => {
                log::debug!("notifications tile clicked");
                sender
                    .output(NotificationsTileOutput::ToggleNotificationCenter)
                    .unwrap_or_else(|_| {
                        log::error!("couldn't send output to open notification center")
                    });
            }
            NotificationsTileMsg::ServiceUpdate(output) => {
                match output {
                    NotificationWorkerOutput::Notifications(notifications) => {
                        let count = notifications.len() as u32;
                        self.active_notifications = notifications;

                        if count != self.notification_count {
                            self.notification_count = count;
                        }
                    }
                    NotificationWorkerOutput::NotificationReceived(notification) => {
                        log::debug!("new notification received: {}", notification.id);

                        // store the notification
                        self.active_notifications
                            .insert(notification.id, notification.clone());

                        // show in fresh notifications panel
                        self.fresh_panel
                            .emit(FreshNotificationsMsg::NewNotification(notification.clone()));

                        // refresh notifications count
                        self.notification_worker
                            .emit(NotificationServiceMsg::GetNotifications);
                    }
                    NotificationWorkerOutput::NotificationClosed { id, reason: _ } => {
                        log::debug!("notification {} closed", id);

                        // remove from active notifications
                        self.active_notifications.remove(&id);

                        // remove from panel
                        self.fresh_panel
                            .emit(FreshNotificationsMsg::RemoveNotification(id));

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
            NotificationsTileMsg::NotificationDismissed(id) => self
                .notification_worker
                .emit(NotificationServiceMsg::CloseNotification(id)),
            NotificationsTileMsg::ActionTriggered(id, action) => self
                .notification_worker
                .emit(NotificationServiceMsg::ActionInvoked(id, action)),
            NotificationsTileMsg::Nothing => (),
        }
    }

    fn update_view(&self, widgets: &mut Self::Widgets, _sender: ComponentSender<Self>) {
        widgets.root.set_visible(true);

        // update tile appearance based on notification count
        let icon = if self.notification_count > 0 {
            ALERT_BADGE_REGULAR
        } else {
            ALERT_REGULAR
        };

        let primary_text = if self.notification_count > 0 {
            Some(self.notification_count.to_string())
        } else {
            None
        };

        let attention = if self.notification_count > 0 {
            Attention::Normal
        } else {
            Attention::Dim
        };

        widgets.tile.emit(TileMsg::SetIcon(Some(icon.to_string())));
        widgets.tile.emit(TileMsg::SetPrimary(primary_text));
        widgets.tile.emit(TileMsg::SetAttention(attention));
    }

    fn init_root() -> Self::Root {
        gtk::Box::builder().visible(false).build()
    }
}
