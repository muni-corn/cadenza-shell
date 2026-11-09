use relm4::component::Connector;

use crate::{
    icon_names::BELL,
    notifications::{NOTIFICATIONS_STATE, NotificationsState},
    tiles::Attention,
    widgets::tile::{Tile, TileState, state_tile},
};

/// The bell, with a badge counting the notifications in the action panel.
///
/// Clicks come out as the tile's output; the owning bar toggles its action
/// panel on them.
pub fn notifications_tile() -> Connector<Tile> {
    state_tile(&NOTIFICATIONS_STATE, view)
}

fn view(state: &NotificationsState) -> Option<TileState> {
    let count = state.notifications.len();
    Some(TileState {
        icon: Some(BELL.to_string()),
        primary: count_text(count),
        attention: count_attention(count),
        ..Default::default()
    })
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
