use gtk4::prelude::*;
use relm4::prelude::*;

use crate::{
    bluetooth::{BLUETOOTH_STATE, BluetoothState},
    bluetooth_menu::BluetoothMenu,
    icon_names::{BLUETOOTH, BLUETOOTH_DOTS, BLUETOOTH_NO, BLUETOOTH_X},
    widgets::tile::{Tile, TileMsg, TileOutput},
};

#[derive(Debug)]
pub struct BluetoothTile {
    tile: Controller<Tile>,
    bluetooth_info: Option<BluetoothState>,
}

#[derive(Debug)]
pub struct BluetoothWidgets {
    _popover: gtk::Popover,
    _menu: Controller<BluetoothMenu>,
}

#[derive(Debug)]
pub enum BluetoothTileMsg {
    Update(Option<BluetoothState>),
}

impl Component for BluetoothTile {
    type CommandOutput = ();
    type Init = ();
    type Input = BluetoothTileMsg;
    type Output = TileOutput;
    type Root = gtk::Box;
    type Widgets = BluetoothWidgets;

    fn init(
        _init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        BLUETOOTH_STATE.subscribe_optional(sender.input_sender(), |state| {
            Some(BluetoothTileMsg::Update(state.to_owned()))
        });

        let current_state = BLUETOOTH_STATE.read().clone();

        // initialize the tile component
        let tile = Tile::builder().launch(Default::default()).detach();

        // initialize the bluetooth menu component
        let bluetooth_menu = BluetoothMenu::builder().launch(()).detach();

        // create the popover
        let popover = gtk::Popover::builder()
            .child(bluetooth_menu.widget())
            .width_request(384)
            .height_request(256)
            .autohide(true)
            .build();
        popover.set_parent(tile.widget());

        // connect click handler to toggle popover
        let popover_clone = popover.clone();
        tile.widget().connect_clicked(move |_| {
            if popover_clone.is_visible() {
                popover_clone.popdown();
            } else {
                popover_clone.popup();
            }
        });

        root.append(tile.widget());

        ComponentParts {
            model: Self {
                tile,
                bluetooth_info: current_state,
            },
            widgets: BluetoothWidgets {
                _popover: popover,
                _menu: bluetooth_menu,
            },
        }
    }

    fn update(
        &mut self,
        BluetoothTileMsg::Update(info): Self::Input,
        _sender: ComponentSender<Self>,
        root: &Self::Root,
    ) {
        root.set_visible(info.is_some());
        self.bluetooth_info = info.clone();

        if let Some(state) = info {
            self.tile
                .emit(TileMsg::SetIcon(Some(get_bluetooth_icon(&state))));
            self.tile
                .emit(TileMsg::SetTooltip(Some(get_tooltip_text(&state))));
        }
    }

    fn init_root() -> Self::Root {
        gtk::Box::new(gtk::Orientation::Horizontal, 0)
    }
}

fn get_bluetooth_icon(state: &BluetoothState) -> String {
    // matches bluetooth_menu's four-state icon mapping; this previously
    // only had three states and never showed a discovering indicator
    if !state.powered {
        BLUETOOTH_NO
    } else if state.discovering {
        BLUETOOTH_DOTS
    } else if state.connected_device_count() > 0 {
        BLUETOOTH
    } else {
        BLUETOOTH_X
    }
    .to_string()
}

/// Builds the tile's tooltip text from cached device snapshots.
///
/// Synchronous: device connection state and alias are already snapshotted
/// in `BluetoothState`, so this no longer needs a per-device D-Bus round
/// trip (previously wrapped in a `oneshot_command` for exactly that reason).
fn get_tooltip_text(state: &BluetoothState) -> String {
    if !state.powered {
        return "Bluetooth disabled".to_string();
    }

    let mut text = String::from("Bluetooth enabled");

    for device in state.devices() {
        if device.connected {
            text.push_str(&format!("\n{}", device.alias));
        }
    }

    text
}
