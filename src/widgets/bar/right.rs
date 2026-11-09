use std::sync::Arc;

use gdk4::Monitor;
use gtk4::prelude::BoxExt;
use relm4::prelude::*;

use crate::{
    settings::BarConfig,
    tiles::{
        battery::battery_tile, bluetooth::BluetoothTile, brightness::brightness_tile,
        network::NetworkTile, notifications::notifications_tile, pulseaudio::volume_tile,
        tray::TrayWidget,
    },
    widgets::{
        progress_tile::ProgressTile,
        tile::{Tile, TileOutput},
        tray_item::TrayClient,
    },
};

#[derive(Debug)]
pub struct RightGroup;

#[derive(Debug)]
pub struct RightWidgets {
    _brightness: Controller<ProgressTile>,
    _volume: Controller<ProgressTile>,
    _bluetooth: Controller<BluetoothTile>,

    _network: Controller<NetworkTile>,
    _battery: Controller<Tile>,
    _notifications: Controller<Tile>,
    _tray: Option<Controller<TrayWidget>>,
}

pub struct RightGroupInit {
    pub bar_config: BarConfig,
    pub tray_client: Option<Arc<TrayClient>>,
    /// The monitor this bar (and thus this group) belongs to, so tiles with
    /// their own layer-shell menu windows (wifi, bluetooth) can pin those
    /// windows to the correct output.
    pub monitor: Monitor,
}

#[derive(Debug)]
pub enum RightGroupOutput {
    ToggleActionPanel,
}

impl SimpleComponent for RightGroup {
    type Init = RightGroupInit;
    type Input = ();
    type Output = RightGroupOutput;
    type Root = gtk::Box;
    type Widgets = RightWidgets;

    fn init_root() -> Self::Root {
        gtk::Box::new(gtk::Orientation::Horizontal, 0)
    }

    fn init(
        RightGroupInit {
            bar_config,
            tray_client,
            monitor,
        }: Self::Init,
        root: Self::Root,
        sender: relm4::ComponentSender<Self>,
    ) -> relm4::ComponentParts<Self> {
        root.set_spacing(bar_config.tile_spacing);
        root.set_margin_horizontal(bar_config.edge_padding);

        let brightness = brightness_tile();
        let volume = volume_tile();
        let bluetooth = BluetoothTile::builder().launch(monitor.clone()).detach();
        let network = NetworkTile::builder().launch(monitor.clone()).detach();
        let battery = battery_tile();
        let notifications = notifications_tile()
            .forward(sender.output_sender(), |TileOutput::Clicked| {
                RightGroupOutput::ToggleActionPanel
            });
        let tray = tray_client.map(|client| TrayWidget::builder().launch(client).detach());

        root.append(brightness.widget());
        root.append(volume.widget());
        root.append(bluetooth.widget());
        root.append(network.widget());
        root.append(battery.widget());
        if let Some(tray) = &tray {
            root.append(tray.widget());
        }
        root.append(notifications.widget());

        ComponentParts {
            model: RightGroup,
            widgets: RightWidgets {
                _brightness: brightness,
                _volume: volume,
                _bluetooth: bluetooth,
                _network: network,
                _battery: battery,
                _notifications: notifications,
                _tray: tray,
            },
        }
    }
}
