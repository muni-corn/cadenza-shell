use gtk4::prelude::*;
use relm4::prelude::*;

use crate::{
    icon_names::BELL,
    notifications::{NOTIFICATIONS_STATE, NotificationsState},
    tiles::Attention,
    widgets::tile::{Tile, TileMsg, TileOutput, TileState},
};

#[derive(Debug)]
pub struct NotificationsTile {
    notification_count: usize,
}

#[derive(Debug)]
pub enum NotificationsTileMsg {
    TileClicked,
    StateUpdate(NotificationsState),
}

#[derive(Debug)]
pub enum NotificationsTileOutput {
    /// The bell was clicked; the owning bar toggles its action panel.
    ToggleActionPanel,
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
                .launch(Some(TileState {
                    icon: Some(BELL.to_string()),
                    primary: count_text(notification_count),
                    attention: count_attention(notification_count),
                    ..Default::default()
                }))
                .forward(sender.input_sender(), |TileOutput::Clicked| {
                    NotificationsTileMsg::TileClicked
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
                    .output(NotificationsTileOutput::ToggleActionPanel)
                    .unwrap_or_else(|_| {
                        tracing::error!("couldn't send output to toggle action panel")
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
        }
    }

    fn update_view(&self, widgets: &mut Self::Widgets, _sender: ComponentSender<Self>) {
        widgets
            .tile
            .emit(TileMsg::SetPrimary(count_text(self.notification_count)));
        widgets.tile.emit(TileMsg::SetAttention(count_attention(
            self.notification_count,
        )));
    }

    fn init_root() -> Self::Root {
        gtk::Box::builder().visible(true).build()
    }
}

/// The badge text: the count, or nothing when there are no notifications.
fn count_text(count: usize) -> Option<String> {
    (count > 0).then(|| count.to_string())
}

/// The bell is dimmed when there's nothing to see.
fn count_attention(count: usize) -> Attention {
    if count > 0 {
        Attention::Normal
    } else {
        Attention::Dim
    }
}
