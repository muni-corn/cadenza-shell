use gtk4::prelude::BoxExt;
use relm4::prelude::*;

use crate::tiles::{
    battery::BatteryTile, bluetooth::BluetoothTile, brightness::BrightnessTile,
    network::NetworkTile, volume::VolumeTile,
};

#[derive(Debug)]
pub struct RightGroup;

#[derive(Debug)]
pub struct RightWidgets {
    _brightness: Controller<BrightnessTile>,
    _volume: Controller<VolumeTile>,
    _bluetooth: Controller<BluetoothTile>,
    _network: Controller<NetworkTile>,
    _battery: Controller<BatteryTile>,
}

impl SimpleComponent for RightGroup {
    type Init = BarConfig;
    type Input = ();
    type Output = ();
    type Root = gtk::Box;
    type Widgets = RightWidgets;

    fn init_root() -> Self::Root {
        gtk::Box::new(gtk::Orientation::Horizontal, 0)
    }

    fn init(
        bar_config: Self::Init,
        root: Self::Root,
        _sender: relm4::ComponentSender<Self>,
    ) -> relm4::ComponentParts<Self> {
        root.set_spacing(bar_config.tile_spacing);
        root.set_margin_horizontal(bar_config.edge_padding);

        let brightness = BrightnessTile::builder().launch(()).detach();
        let volume = VolumeTile::builder().launch(()).detach();
        let bluetooth = BluetoothTile::builder().launch(()).detach();
        let network = NetworkTile::builder().launch(()).detach();
        let battery = BatteryTile::builder().launch(()).detach();

        root.append(brightness.widget());
        root.append(volume.widget());
        root.append(bluetooth.widget());
        root.append(network.widget());
        root.append(battery.widget());

        ComponentParts {
            model: RightGroup,
            widgets: RightWidgets {
                _brightness: brightness,
                _volume: volume,
                _bluetooth: bluetooth,
                _network: network,
                _battery: battery,
            },
        }
    }
}
