use gtk4::prelude::*;
use relm4::{WorkerController, prelude::*};

use crate::{
    icon_names::{BLUETOOTH_CONNECTED_REGULAR, BLUETOOTH_DISABLED_REGULAR, BLUETOOTH_REGULAR},
    services::bluetooth::{BluetoothInfo, BluetoothService, BluetoothWorkerOutput},
    widgets::tile::{Tile, TileMsg, TileOutput},
};

#[derive(Debug)]
pub struct BluetoothTile {
    bluetooth_info: BluetoothInfo,
    _worker: WorkerController<BluetoothService>,
}

#[derive(Debug)]
pub struct BluetoothWidgets {
    root: gtk::Box,
    tile: Controller<Tile>,
}

impl SimpleComponent for BluetoothTile {
    type Init = ();
    type Input = BluetoothInfo;
    type Output = TileOutput;
    type Root = gtk::Box;
    type Widgets = BluetoothWidgets;

    fn init(
        _init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        // Initialize the Tile component
        let tile = Tile::builder().launch(Default::default()).detach();

        root.append(tile.widget());

        // Start Bluetooth service worker
        let bluetooth_worker = BluetoothService::builder().detach_worker(()).forward(
            sender.input_sender(),
            |output| match output {
                BluetoothWorkerOutput::StateChanged(info) => info,
            },
        );

        let model = BluetoothTile {
            bluetooth_info: BluetoothInfo::default(),
            _worker: bluetooth_worker,
        };

        ComponentParts {
            model,
            widgets: BluetoothWidgets { root, tile },
        }
    }

    fn update(&mut self, info: Self::Input, _sender: ComponentSender<Self>) {
        self.bluetooth_info = info;
    }

    fn update_view(&self, widgets: &mut Self::Widgets, _sender: ComponentSender<Self>) {
        widgets.root.set_visible(self.bluetooth_info.available);
        widgets.tile.emit(TileMsg::SetIcon(Some(self.get_icon())));
    }

    fn init_root() -> Self::Root {
        gtk::Box::new(gtk::Orientation::Horizontal, 0)
    }
}

impl BluetoothTile {
    fn get_icon(&self) -> String {
        if !self.bluetooth_info.available {
            BLUETOOTH_DISABLED_REGULAR
        } else if self.bluetooth_info.enabled {
            if self.bluetooth_info.connected_devices > 0 {
                BLUETOOTH_CONNECTED_REGULAR
            } else {
                BLUETOOTH_REGULAR
            }
        } else {
            BLUETOOTH_DISABLED_REGULAR
        }
        .to_string()
    }
}
