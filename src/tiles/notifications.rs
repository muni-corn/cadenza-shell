use gtk4::prelude::*;
use relm4::prelude::*;

use crate::{
    icon_names::BELL,
    notifications::{NOTIFICATIONS_STATE, NotificationsState},
    tiles::Attention,
    widgets::tile::{Tile, TileInit, TileMsg, TileOutput},
};

#[derive(Debug)]
pub struct NotificationsTile {
    notification_count: usize,
}

#[derive(Debug)]
pub enum NotificationsTileMsg {
    TileClicked,
    StateUpdate(NotificationsState),
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

        let model = NotificationsTile { notification_count };

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            NotificationsTileMsg::TileClicked => {
                tracing::debug!("notifications tile clicked");
                sender
                    .output(NotificationsTileOutput::ToggleNotificationCenter)
                    .unwrap_or_else(|_| {
                        tracing::error!("couldn't send output to open notification center")
                    });
            }
            NotificationsTileMsg::StateUpdate(state) => {
                let new_count = state.notifications.len();
                tracing::debug!(
                    previous_count = self.notification_count,
                    notification_count = new_count,
                    "tile state update"
                );
                self.notification_count = new_count;
            }
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
