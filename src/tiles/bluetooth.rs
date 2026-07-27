use gdk4::Monitor;
use gtk4::prelude::*;
use relm4::prelude::*;

use crate::{
    bluetooth::{BLUETOOTH_STATE, BluetoothState},
    bluetooth_menu::BluetoothMenu,
    icon_names::{BLUETOOTH, BLUETOOTH_DOTS, BLUETOOTH_NO, BLUETOOTH_X},
    widgets::{
        menu_window::{MenuWindow, MenuWindowInit, MenuWindowMsg, MenuWindowOutput},
        tile::{Tile, TileMsg, TileOutput},
    },
};

#[derive(Debug)]
pub struct BluetoothTile {
    tile: Controller<Tile>,
    bluetooth_info: Option<BluetoothState>,
    menu_open: bool,
    menu_window: Controller<MenuWindow>,
}

#[derive(Debug)]
pub struct BluetoothWidgets {
    _menu: Controller<BluetoothMenu>,
}

#[derive(Debug)]
pub enum BluetoothTileMsg {
    Update(Option<BluetoothState>),
    ToggleMenu,
    MenuClosed,
}

impl Component for BluetoothTile {
    type CommandOutput = ();
    type Init = Monitor;
    type Input = BluetoothTileMsg;
    type Output = TileOutput;
    type Root = gtk::Box;
    type Widgets = BluetoothWidgets;

    fn init(
        monitor: Self::Init,
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
            .forward(sender.input_sender(), |output| match output {
                MenuWindowOutput::Hidden => BluetoothTileMsg::MenuClosed,
            });

        // toggle the menu window on click; authoritative for open/closed
        // state so it doesn't depend on the window's own visibility
        tile.widget().connect_clicked({
            let sender = sender.clone();
            move |_| sender.input(BluetoothTileMsg::ToggleMenu)
        });

        root.append(tile.widget());

        ComponentParts {
            model: Self {
                tile,
                bluetooth_info: current_state,
                menu_open: false,
                menu_window,
            },
            widgets: BluetoothWidgets {
                _menu: bluetooth_menu,
            },
        }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>, root: &Self::Root) {
        match msg {
            BluetoothTileMsg::Update(info) => {
                root.set_visible(info.is_some());
                self.bluetooth_info = info.clone();

                if let Some(state) = info {
                    self.tile
                        .emit(TileMsg::SetIcon(Some(get_bluetooth_icon(&state))));
                    self.tile
                        .emit(TileMsg::SetTooltip(Some(get_tooltip_text(&state))));
                }
            }
            BluetoothTileMsg::ToggleMenu => {
                self.menu_open = !self.menu_open;
                self.menu_window.emit(if self.menu_open {
                    MenuWindowMsg::Show
                } else {
                    MenuWindowMsg::Hide
                });
            }
            BluetoothTileMsg::MenuClosed => {
                self.menu_open = false;
            }
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
