use gtk4::prelude::*;
use relm4::prelude::*;

use crate::{
    icon_names::BELL,
    notifications::{
        NOTIFICATIONS_STATE, NotificationEvent, NotificationsState,
        fresh::{FreshNotifications, FreshNotificationsMsg, FreshNotificationsOutput},
        subscribe_events,
    },
    tiles::Attention,
    widgets::tile::{Tile, TileInit, TileMsg, TileOutput},
};

#[derive(Debug)]
pub struct NotificationsTile {
    notification_count: usize,
    fresh_panel: Controller<FreshNotifications>,
}

#[derive(Debug)]
pub enum NotificationsTileMsg {
    TileClicked,
    StateUpdate(NotificationsState),
    Event(NotificationEvent),
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
        // subscribe to snapshot state for the count badge
        NOTIFICATIONS_STATE.subscribe(sender.input_sender(), |s| {
            NotificationsTileMsg::StateUpdate(s.clone())
        });

        // subscribe to per-event stream for driving fresh popups
        let event_rx = subscribe_events();
        let event_sender = sender.input_sender().clone();
        relm4::spawn(async move {
            let mut rx = event_rx;
            loop {
                match rx.recv().await {
                    Ok(event) => {
                        event_sender
                            .send(NotificationsTileMsg::Event(event))
                            .unwrap_or_else(|_| {
                                log::error!("couldn't forward notification event to tile")
                            });
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        log::warn!("notifications tile missed {n} events (lagged receiver)");
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        log::warn!("notifications event channel closed; tile event loop stopping");
                        break;
                    }
                }
            }
        });

        // create popups for first monitor only
        let display = gdk4::Display::default().expect("could not get default display");
        let monitor = display
            .monitors()
            .iter()
            .next()
            .expect("no monitor available for notifications")
            .expect("couldn't get available monitor for notifications");

        let fresh_panel =
            FreshNotifications::builder()
                .launch(monitor)
                .forward(sender.input_sender(), |msg| match msg {
                    FreshNotificationsOutput::NotificationDismissed(id) => {
                        crate::notifications::dismiss(id);
                        NotificationsTileMsg::Nothing
                    }
                    FreshNotificationsOutput::NotificationActionTriggered(id, action) => {
                        crate::notifications::invoke_action(id, action);
                        NotificationsTileMsg::Nothing
                    }
                });

        let notification_count = NOTIFICATIONS_STATE.read().notifications.len();

        let widgets = NotificationsTileWidgets {
            root,
            tile: Tile::builder()
                .launch(TileInit {
                    icon_name: Some(BELL.to_string()),
                    ..Default::default()
                })
                .forward(sender.input_sender(), |msg| match msg {
                    TileOutput::Clicked => NotificationsTileMsg::TileClicked,
                    _ => NotificationsTileMsg::Nothing,
                }),
        };

        widgets.root.append(widgets.tile.widget());

        let model = NotificationsTile {
            notification_count,
            fresh_panel,
        };

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
            NotificationsTileMsg::StateUpdate(state) => {
                self.notification_count = state.notifications.len();
            }
            NotificationsTileMsg::Event(event) => match event {
                NotificationEvent::Received(notification) => {
                    log::debug!("new notification received: {}", notification.id);
                    self.fresh_panel
                        .emit(FreshNotificationsMsg::NewNotification(notification));
                }
                NotificationEvent::Closed { id, .. } => {
                    log::debug!("notification {} closed", id);
                    self.fresh_panel
                        .emit(FreshNotificationsMsg::RemoveNotification(id));
                }
                NotificationEvent::AllCleared => {
                    // fresh panel will drain as each close event arrives via
                    // the state update; no extra action needed here
                }
                NotificationEvent::ActionInvoked { .. } => {}
            },
            NotificationsTileMsg::Nothing => {}
        }
    }

    fn update_view(&self, widgets: &mut Self::Widgets, _sender: ComponentSender<Self>) {
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

        widgets.tile.emit(TileMsg::SetPrimary(primary_text));
        widgets.tile.emit(TileMsg::SetAttention(attention));
    }

    fn init_root() -> Self::Root {
        gtk::Box::builder().visible(true).build()
    }
}
