use gtk4::prelude::BoxExt;
use relm4::prelude::*;

use crate::{
    settings::BarConfig,
    tiles::{
        battery::BatteryTile,
        bluetooth::BluetoothTile,
        brightness::BrightnessTile,
        network::NetworkTile,
        notifications::{NotificationsTile, NotificationsTileOutput},
        pulseaudio::PulseAudioTile,
        tray::TrayWidget,
    },
};

#[derive(Debug)]
pub struct RightGroup;

pub struct RightWidgets {
    _brightness: Controller<BrightnessTile>,
    _volume: Controller<PulseAudioTile>,
    _bluetooth: Controller<BluetoothTile>,
    _network: Controller<NetworkTile>,
    _battery: Controller<BatteryTile>,
    _notifications: Controller<NotificationsTile>,
    _tray: Controller<TrayWidget>,
}

#[derive(Debug)]
pub enum RightGroupOutput {
    ToggleNotificationCenter,
}

impl SimpleComponent for RightGroup {
    type Init = BarConfig;
    type Input = ();
    type Output = RightGroupOutput;
    type Root = gtk::Box;
    type Widgets = RightWidgets;

    fn init_root() -> Self::Root {
        gtk::Box::new(gtk::Orientation::Horizontal, 0)
    }

    fn init(
        bar_config: Self::Init,
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
        let tray = TrayWidget::builder().launch(()).detach();

        root.append(brightness.widget());
        root.append(volume.widget());
        root.append(bluetooth.widget());
        root.append(network.widget());
        root.append(battery.widget());
        root.append(notifications.widget());
        root.append(tray.widget());

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
