use gtk4::prelude::*;
use relm4::prelude::*;

use crate::{
    bluetooth::{BLUETOOTH_STATE, BluetoothState},
    icon_names::{BLUETOOTH_CONNECTED_REGULAR, BLUETOOTH_DISABLED_REGULAR, BLUETOOTH_REGULAR},
    widgets::tile::{Tile, TileMsg, TileOutput},
};

#[derive(Debug)]
pub struct BluetoothTile {
    bluetooth_info: Option<BluetoothState>,
}

#[derive(Debug)]
pub struct BluetoothWidgets {
    root: gtk::Box,
    tile: Controller<Tile>,
}

impl SimpleComponent for BluetoothTile {
    type Init = ();
    type Input = BluetoothState;
    type Output = TileOutput;
    type Root = gtk::Box;
    type Widgets = BluetoothWidgets;

    fn init(
        _init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        BLUETOOTH_STATE.subscribe_optional(sender.input_sender(), |state| state.to_owned());

        // initialize the tile component
        let tile = Tile::builder().launch(Default::default()).detach();

        root.append(tile.widget());

        ComponentParts {
            model: Self {
                bluetooth_info: None,
            },
            widgets: BluetoothWidgets { root, tile },
        }
    }

    fn update(&mut self, info: Self::Input, _sender: ComponentSender<Self>) {
        self.bluetooth_info = Some(info);
    }

    fn update_view(&self, widgets: &mut Self::Widgets, _sender: ComponentSender<Self>) {
        if let Some(ref state) = self.bluetooth_info {
            widgets.root.set_visible(true);
            widgets
                .tile
                .emit(TileMsg::SetIcon(Some(get_bluetooth_icon(state))));
        } else {
            widgets.root.set_visible(false);
        }
    }

    fn init_root() -> Self::Root {
        gtk::Box::new(gtk::Orientation::Horizontal, 0)
    }
}

fn get_bluetooth_icon(state: &BluetoothState) -> String {
    if state.powered {
        if state.connected_device_count > 0 {
            BLUETOOTH_CONNECTED_REGULAR
        } else {
            BLUETOOTH_REGULAR
        }
    } else {
        BLUETOOTH_DISABLED_REGULAR
    }
    .to_string()
}
