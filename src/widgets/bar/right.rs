use std::sync::{Arc, Mutex};

use gdk4::Monitor;
use gtk4::prelude::BoxExt;
use relm4::prelude::*;
use system_tray::{client::Event as TrayEvent, data::BaseMap};

use crate::{
    settings::BarConfig,
    tiles::{
        battery::battery_tile,
        bluetooth::BluetoothTile,
        brightness::BrightnessTile,
        network::NetworkTile,
        notifications::notifications_tile,
        pulseaudio::PulseAudioTile,
        tray::{TrayMsg, TrayWidget},
    },
    widgets::{
        tile::{Tile, TileOutput},
        tray_item::TrayItemOutput,
    },
};

#[derive(Debug)]
pub struct RightGroup {
    tray: Option<Controller<TrayWidget>>,
}

#[derive(Debug)]
pub struct RightWidgets {
    _brightness: Controller<BrightnessTile>,
    _volume: Controller<PulseAudioTile>,
    _bluetooth: Controller<BluetoothTile>,

    _network: Controller<NetworkTile>,
    _battery: Controller<Tile>,
    _notifications: Controller<Tile>,
}

pub struct RightGroupInit {
    pub bar_config: BarConfig,
    pub tray_items: Option<Arc<Mutex<BaseMap>>>,
    /// The monitor this bar (and thus this group) belongs to, so tiles with
    /// their own layer-shell menu windows (wifi, bluetooth) can pin those
    /// windows to the correct output.
    pub monitor: Monitor,
}

#[derive(Debug)]
pub enum RightGroupMsg {
    TrayEvent(TrayEvent),
}

#[derive(Debug)]
pub enum RightGroupOutput {
    ToggleActionPanel,
    TrayItemOutput(TrayItemOutput),
}

impl SimpleComponent for RightGroup {
    type Init = RightGroupInit;
    type Input = RightGroupMsg;
    type Output = RightGroupOutput;
    type Root = gtk::Box;
    type Widgets = RightWidgets;

    fn init_root() -> Self::Root {
        gtk::Box::new(gtk::Orientation::Horizontal, 0)
    }

    fn init(
        RightGroupInit {
            bar_config,
            tray_items,
            monitor,
        }: Self::Init,
        root: Self::Root,
        sender: relm4::ComponentSender<Self>,
    ) -> relm4::ComponentParts<Self> {
        root.set_spacing(bar_config.tile_spacing);
        root.set_margin_horizontal(bar_config.edge_padding);

        let brightness = BrightnessTile::builder().launch(()).detach();
        let volume = PulseAudioTile::builder().launch(()).detach();
        let bluetooth = BluetoothTile::builder().launch(monitor.clone()).detach();
        let network = NetworkTile::builder().launch(monitor.clone()).detach();
        let battery = battery_tile();
        let notifications = notifications_tile()
            .forward(sender.output_sender(), |TileOutput::Clicked| {
                RightGroupOutput::ToggleActionPanel
            });
        let tray_opt = tray_items.and_then(|m| match m.lock() {
            Ok(items) => Some(
                TrayWidget::builder()
                    .launch(items.clone())
                    .forward(sender.output_sender(), RightGroupOutput::TrayItemOutput),
            ),
            Err(e) => {
                tracing::error!("couldn't lock tray items mutex: {}", e);
                None
            }
        });

        root.append(brightness.widget());
        root.append(volume.widget());
        root.append(bluetooth.widget());
        root.append(network.widget());
        root.append(battery.widget());
        if let Some(tray) = &tray_opt {
            root.append(tray.widget());
        }
        root.append(notifications.widget());

        ComponentParts {
            model: RightGroup { tray: tray_opt },
            widgets: RightWidgets {
                _brightness: brightness,
                _volume: volume,
                _bluetooth: bluetooth,
                _network: network,
                _battery: battery,
                _notifications: notifications,
            },
        }
    }

    fn update(&mut self, message: Self::Input, _sender: ComponentSender<Self>) {
        match message {
            RightGroupMsg::TrayEvent(event) => {
                if let Some(ref tray) = self.tray {
                    tray.emit(TrayMsg::TrayEvent(event));
                }
            }
        }
    }
}
