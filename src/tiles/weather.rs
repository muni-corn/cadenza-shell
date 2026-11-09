use relm4::prelude::*;

use crate::{
    weather::{WEATHER_STATE, types::WeatherState},
    widgets::tile::{Tile, TileState, state_tile},
};

/// The current temperature and conditions, hidden until weather is known.
pub fn weather_tile() -> Controller<Tile> {
    state_tile(&WEATHER_STATE, view).detach()
}

fn view(weather: &Option<WeatherState>) -> Option<TileState> {
    let weather = weather.as_ref()?;
    Some(TileState {
        icon: Some(weather.icon.to_string()),
        primary: Some(format!("{}°", weather.temperature)),
        secondary: Some(weather.condition.clone()),
        ..Default::default()
    })
}
