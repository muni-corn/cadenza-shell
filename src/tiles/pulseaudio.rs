use relm4::Controller;

use crate::{
    pulseaudio::{PulseAudioData, VOLUME_STATE},
    utils::icons::{VOLUME_ICONS, VOLUME_MUTED, VOLUME_ZERO, percentage_to_icon_from_list},
    widgets::progress_tile::{ProgressState, ProgressTile, state_progress_tile},
};

/// The default sink's volume, hidden without a sound server.
pub fn volume_tile() -> Controller<ProgressTile> {
    state_progress_tile(&VOLUME_STATE, view)
}

fn view(volume: &PulseAudioData) -> Option<ProgressState> {
    volume.default_sink_name.as_ref()?;

    let progress = if volume.muted {
        0.0
    } else {
        (volume.volume / 100.0).clamp(0.0, 1.0)
    };
    Some(ProgressState {
        icon: get_icon(volume).to_string(),
        progress,
    })
}

fn get_icon(volume: &PulseAudioData) -> &'static str {
    if volume.muted {
        VOLUME_MUTED
    } else if volume.volume == 0.0 {
        VOLUME_ZERO
    } else {
        percentage_to_icon_from_list(volume.volume / 100.0, VOLUME_ICONS)
    }
}
