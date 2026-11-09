//! The tiles shown in a bar, laid out in its start, center and end groups.

use std::sync::Arc;

use gdk4::Monitor;
use gtk4::prelude::*;
use relm4::{RelmWidgetExt, Sender, prelude::*};

use crate::{
    settings::BarConfig,
    tiles::{
        battery::battery_tile,
        bluetooth::BluetoothTile,
        brightness::brightness_tile,
        clock::ClockTile,
        mpris::mpris_tile,
        network::NetworkTile,
        niri::{NiriInit, NiriTile},
        notifications::notifications_tile,
        pulseaudio::volume_tile,
        tray::TrayWidget,
        weather::weather_tile,
    },
    widgets::{
        bar::BarMsg,
        progress_tile::ProgressTile,
        tile::{Tile, TileOutput},
        tray_item::TrayClient,
    },
};

/// The tile controllers of one bar, held so the tiles live as long as it.
#[derive(Debug)]
pub struct BarTiles {
    _niri: Controller<NiriTile>,
    _clock: Controller<ClockTile>,
    _weather: Controller<Tile>,
    _media: Controller<Tile>,
    _brightness: Controller<ProgressTile>,
    _volume: Controller<ProgressTile>,
    _bluetooth: Controller<BluetoothTile>,
    _network: Controller<NetworkTile>,
    _battery: Controller<Tile>,
    _notifications: Controller<Tile>,
    _tray: Option<Controller<TrayWidget>>,
}

/// The boxes a bar lays its tiles out in.
pub struct BarGroups {
    pub start: gtk::Box,
    pub center: gtk::Box,
    pub end: gtk::Box,
}

impl BarTiles {
    /// Launches the tiles for the bar on `monitor` and lays them out.
    ///
    /// The notifications tile toggles the bar's action panel through `bar`.
    pub fn launch(
        config: BarConfig,
        monitor: &Monitor,
        tray_client: Option<Arc<TrayClient>>,
        bar: &Sender<BarMsg>,
    ) -> (Self, BarGroups) {
        let tiles = BarTiles {
            _niri: NiriTile::builder()
                .launch(NiriInit {
                    bar_config: config,
                    monitor: monitor.clone(),
                })
                .detach(),
            _clock: ClockTile::builder().launch(()).detach(),
            _weather: weather_tile(),
            _media: mpris_tile(),
            _brightness: brightness_tile(),
            _volume: volume_tile(),
            _bluetooth: BluetoothTile::builder().launch(monitor.clone()).detach(),
            _network: NetworkTile::builder().launch(monitor.clone()).detach(),
            _battery: battery_tile(),
            _notifications: notifications_tile()
                .forward(bar, |TileOutput::Clicked| BarMsg::ToggleActionPanel),
            _tray: tray_client.map(|client| TrayWidget::builder().launch(client).detach()),
        };

        let groups = BarGroups {
            start: group(config, &[tiles._niri.widget().upcast_ref()]),
            center: group(config, &[
                tiles._clock.widget().upcast_ref(),
                tiles._weather.widget().upcast_ref(),
                tiles._media.widget().upcast_ref(),
            ]),
            end: group(config, &[
                tiles._brightness.widget().upcast_ref(),
                tiles._volume.widget().upcast_ref(),
                tiles._bluetooth.widget().upcast_ref(),
                tiles._network.widget().upcast_ref(),
                tiles._battery.widget().upcast_ref(),
            ]),
        };

        // the tray sits just before the bell, at the very end
        if let Some(tray) = &tiles._tray {
            groups.end.append(tray.widget());
        }
        groups.end.append(tiles._notifications.widget());

        (tiles, groups)
    }
}

/// A horizontally padded box holding `tiles` in order.
fn group(config: BarConfig, tiles: &[&gtk::Widget]) -> gtk::Box {
    let group = gtk::Box::new(gtk::Orientation::Horizontal, config.tile_spacing);
    group.set_margin_horizontal(config.edge_padding);
    for tile in tiles {
        group.append(*tile);
    }
    group
}
