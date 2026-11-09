use relm4::prelude::*;

use crate::{
    icon_names::{MEDIA_PLAYBACK_PAUSE, MUSIC_NOTE_SINGLE},
    mpris::{MPRIS_STATE, MprisState, PlaybackStatus},
    widgets::tile::{Tile, TileState, state_tile},
};

/// The track the current media player is on, hidden without a player.
pub fn mpris_tile() -> Controller<Tile> {
    state_tile(&MPRIS_STATE, view).detach()
}

fn view(state: &Option<MprisState>) -> Option<TileState> {
    let state = state.as_ref()?;
    let icon = match state.status {
        PlaybackStatus::Playing => MUSIC_NOTE_SINGLE,
        _ => MEDIA_PLAYBACK_PAUSE,
    };

    Some(TileState {
        icon: Some(icon.to_string()),
        primary: state.title.clone(),
        secondary: state.artist.clone(),
        ..Default::default()
    })
}
