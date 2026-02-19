use std::sync::{Arc, Mutex};

use gtk4::prelude::BoxExt;
use relm4::prelude::*;
use system_tray::{client::Event as TrayEvent, data::BaseMap};

use crate::{
    settings::BarConfig,
    tiles::{
        battery::BatteryTile,
        bluetooth::BluetoothTile,
        brightness::BrightnessTile,
        network::NetworkTile,
        notifications::{NotificationsTile, NotificationsTileOutput},
        pulseaudio::PulseAudioTile,
        tray::{TrayMsg, TrayWidget},
    },
    widgets::tray_item::TrayItemOutput,
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
    _battery: Controller<BatteryTile>,
    _notifications: Controller<NotificationsTile>,
}

pub struct RightGroupInit {
    pub bar_config: BarConfig,
    pub tray_items: Option<Arc<Mutex<BaseMap>>>,
}

#[derive(Debug)]
pub enum RightGroupMsg {
    TrayEvent(TrayEvent),
}

#[derive(Debug)]
pub enum RightGroupOutput {
    ToggleNotificationCenter,
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
        }: Self::Init,
        root: Self::Root,
        sender: relm4::ComponentSender<Self>,
    ) -> relm4::ComponentParts<Self> {
        root.set_spacing(bar_config.tile_spacing);
        root.set_margin_horizontal(bar_config.edge_padding);

        let brightness = BrightnessTile::builder().launch(()).detach();
        let volume = PulseAudioTile::builder().launch(()).detach();
        let bluetooth = BluetoothTile::builder().launch(()).detach();
        let network = NetworkTile::builder().launch(()).detach();
        let battery = BatteryTile::builder().launch(()).detach();
        let notifications = NotificationsTile::builder().launch(()).forward(
            sender.output_sender(),
            |msg| match msg {
                NotificationsTileOutput::ToggleNotificationCenter => {
                    RightGroupOutput::ToggleNotificationCenter
                }
            },
        );
        let tray_opt = tray_items.and_then(|m| match m.lock() {
            Ok(items) => Some(
                TrayWidget::builder()
                    .launch(items.clone())
                    .forward(sender.output_sender(), RightGroupOutput::TrayItemOutput),
            ),
            Err(e) => {
                log::error!("couldn't lock tray items mutex: {}", e);
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
