use gdk4::Monitor;
use gtk4::prelude::*;
use relm4::prelude::*;

use crate::{
    bluetooth::{BLUETOOTH_STATE, BluetoothState, get_icon},
    bluetooth_menu::{BluetoothMenu, BluetoothMenuMsg},
    widgets::{
        menu_window::{MenuWindow, MenuWindowInit, MenuWindowMsg, MenuWindowOutput},
        tile::{Tile, TileOutput, TileState, state_tile},
    },
};

/// The bluetooth tile and the menu it opens.
#[derive(Debug)]
pub struct BluetoothTile {
    menu_window: Controller<MenuWindow>,
    bluetooth_menu: Controller<BluetoothMenu>,
}

#[derive(Debug)]
pub struct BluetoothWidgets {
    _tile: Controller<Tile>,
}

#[derive(Debug)]
pub enum BluetoothTileMsg {
    TileClicked,
    MenuShown(bool),
}

impl SimpleComponent for BluetoothTile {
    type Init = Monitor;
    type Input = BluetoothTileMsg;
    type Output = ();
    type Root = gtk::Box;
    type Widgets = BluetoothWidgets;

    fn init(
        monitor: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let tile = state_tile(&BLUETOOTH_STATE, view)
            .forward(sender.input_sender(), |TileOutput::Clicked| {
                BluetoothTileMsg::TileClicked
            });

        let bluetooth_menu = BluetoothMenu::builder().launch(()).detach();

        // present it in a keyboard-interactive layer-shell window instead of
        // a gtk::Popover, since the bar itself never accepts keyboard focus
        // (needed for typing a pairing PIN)
        let menu_window = MenuWindow::builder()
            .launch(MenuWindowInit {
                namespace: "bluetooth-menu",
                monitor: Some(monitor),
                width: 384,
                content: bluetooth_menu.widget().clone().upcast(),
            })
            .forward(sender.input_sender(), |output| {
                BluetoothTileMsg::MenuShown(matches!(output, MenuWindowOutput::Shown))
            });

        root.append(tile.widget());

        ComponentParts {
            model: Self {
                menu_window,
                bluetooth_menu,
            },
            widgets: BluetoothWidgets { _tile: tile },
        }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            BluetoothTileMsg::TileClicked => self.menu_window.emit(MenuWindowMsg::Toggle),
            BluetoothTileMsg::MenuShown(shown) => {
                self.bluetooth_menu.emit(BluetoothMenuMsg::SetOpen(shown));
            }
        }
    }

    fn init_root() -> Self::Root {
        gtk::Box::new(gtk::Orientation::Horizontal, 0)
    }
}

/// Hidden without an adapter.
fn view(state: &Option<BluetoothState>) -> Option<TileState> {
    let state = state.as_ref()?;
    Some(TileState {
        icon: Some(get_icon(Some(state)).to_string()),
        tooltip: Some(get_tooltip_text(state)),
        ..Default::default()
    })
}

/// Builds the tile's tooltip text from cached device snapshots.
///
/// Synchronous: device connection state and alias are already snapshotted
/// in `BluetoothState`, so no D-Bus round trip is needed.
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
