use relm4::Controller;

use crate::{
    brightness::BRIGHTNESS_STATE,
    utils::icons::{BRIGHTNESS_ICON_NAMES, percentage_to_icon_from_list},
    widgets::progress_tile::{ProgressState, ProgressTile, state_progress_tile},
};

/// The backlight level, hidden without a backlight.
pub fn brightness_tile() -> Controller<ProgressTile> {
    state_progress_tile(&BRIGHTNESS_STATE, view)
}

fn view(brightness: &Option<f64>) -> Option<ProgressState> {
    let icon = brightness
        .map(|p| percentage_to_icon_from_list(p, BRIGHTNESS_ICON_NAMES).to_string())
        .unwrap_or_default();

    Some(ProgressState {
        icon,
        progress: (*brightness)?,
    })
}
