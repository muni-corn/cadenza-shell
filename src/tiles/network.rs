use gtk4::prelude::*;
use relm4::prelude::*;

use crate::{
    icon_names::*,
    network::{
        NETWORK_STATE,
        types::{DeviceType, NetworkState, State},
    },
    utils::icons::{NETWORK_WIFI_ICON_NAMES, percentage_to_icon_from_list},
    widgets::tile::{Tile, TileMsg, TileOutput},
};

#[derive(Debug)]
pub struct NetworkTile;

#[derive(Debug)]
pub enum NetworkTileMsg {
    Click,
    Update,
}

impl SimpleComponent for NetworkTile {
    type Init = ();
    type Input = NetworkTileMsg;
    type Output = TileOutput;
    type Root = gtk::Box;
    type Widgets = Controller<Tile>;

    fn init(
        _init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        NETWORK_STATE.subscribe(sender.input_sender(), |_| NetworkTileMsg::Update);

        // initialize the Tile component
        let tile =
            Tile::builder()
                .launch(Default::default())
                .forward(sender.input_sender(), |output| match output {
                    TileOutput::Clicked => NetworkTileMsg::Click,
                    _ => NetworkTileMsg::Update,
                });

        root.append(tile.widget());

        // initialize
        sender.input(NetworkTileMsg::Update);

        ComponentParts {
            model: Self,
            widgets: tile,
        }
    }

    fn update(&mut self, _msg: Self::Input, _sender: ComponentSender<Self>) {}

    fn update_view(&self, tile: &mut Self::Widgets, _sender: ComponentSender<Self>) {
        let info = &*NETWORK_STATE.read();

        let icon = get_icon(info);

        tile.emit(TileMsg::SetIcon(Some(icon.to_string())));
        tile.emit(TileMsg::SetPrimary(None));
        tile.emit(TileMsg::SetSecondary(
            get_secondary_text(info).map(String::from),
        ));
    }

    fn init_root() -> Self::Root {
        gtk::Box::builder().build()
    }
}

fn get_icon(info: &NetworkState) -> &str {
    if !info.connected {
        return "network-offline-symbolic";
    }

    match info.device_type {
        DeviceType::Wifi => {
            let strength = info.wifi_strength as f64;
            percentage_to_icon_from_list(strength, NETWORK_WIFI_ICON_NAMES)
        }
        _ => EARTH_REGULAR,
    }
}

fn get_secondary_text(info: &NetworkState) -> Option<&str> {
    if !info.connected {
        return Some("Disconnected");
    }

    Some(match info.state {
        State::ConnectedLocal | State::ConnectedGlobal | State::ConnectedSite => return None,
        State::Unknown => "State unknown",
        State::Asleep => "Asleep",
        State::Disconnected => "Disconnected",
        State::Disconnecting => "Disconnecting",
        State::Connecting => "Connecting",
    })
}
