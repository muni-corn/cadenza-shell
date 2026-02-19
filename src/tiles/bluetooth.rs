use gtk4::prelude::*;
use relm4::prelude::*;

use crate::{
    bluetooth::{BLUETOOTH_STATE, BluetoothState},
    icon_names::{BLUETOOTH_CONNECTED_REGULAR, BLUETOOTH_DISABLED_REGULAR, BLUETOOTH_REGULAR},
    widgets::tile::{Tile, TileMsg, TileOutput},
};

#[derive(Debug)]
pub struct BluetoothTile {
    tile: Controller<Tile>,
    bluetooth_info: Option<BluetoothState>,
    tooltip_text: String,
}

#[derive(Debug)]
pub struct BluetoothWidgets {}

#[derive(Debug)]
pub enum BluetoothTileMsg {
    Update(Option<BluetoothState>),
}

#[derive(Debug)]
pub enum BluetoothTileCommandOutput {
    TooltipText(String),
}

impl Component for BluetoothTile {
    type CommandOutput = BluetoothTileCommandOutput;
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

        root.append(tile.widget());

        ComponentParts {
            model: Self {
                tile,
                bluetooth_info: current_state,
                tooltip_text: String::new(),
            },
            widgets: BluetoothWidgets {},
        }
    }

    fn update(
        &mut self,
        BluetoothTileMsg::Update(info): Self::Input,
        sender: ComponentSender<Self>,
        _root: &Self::Root,
    ) {
        self.bluetooth_info = info.clone();

        if let Some(state) = info {
            self.tile
                .emit(TileMsg::SetIcon(Some(get_bluetooth_icon(&state))));
            sender.oneshot_command(async move {
                let text = get_tooltip_text(&state).await;
                BluetoothTileCommandOutput::TooltipText(text)
            });
        }
    }

    fn update_cmd(
        &mut self,
        BluetoothTileCommandOutput::TooltipText(text): Self::CommandOutput,
        _sender: ComponentSender<Self>,
        _root: &Self::Root,
    ) {
        self.tooltip_text = text;
        self.tile
            .emit(TileMsg::SetTooltip(Some(self.tooltip_text.clone())));
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

async fn get_tooltip_text(state: &BluetoothState) -> String {
    if !state.powered {
        return "Bluetooth disabled".to_string();
    }

    let mut text = String::from("Bluetooth enabled");

    for device in state.devices() {
        if device.is_connected().await.unwrap_or(false)
            && let Ok(Some(name)) = device.name().await
        {
            text.push_str(&format!("\n{}", name));
        }
    }

    text
}
