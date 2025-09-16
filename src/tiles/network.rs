use gtk4::prelude::*;
use relm4::{WorkerController, prelude::*};

use crate::{
    icon_names::*,
    services::network::{
        DeviceType, NetworkInfo, NetworkService, NetworkState, NetworkWorkerOutput,
    },
    utils::icons::{NETWORK_WIFI_ICON_NAMES, percentage_to_icon_from_list},
    widgets::tile::{Tile, TileInit, TileMsg, TileOutput},
};

#[derive(Debug)]
pub struct NetworkTile {
    _network_info: NetworkInfo,
    tile: Controller<Tile>,
    _worker: WorkerController<NetworkService>,
}

#[derive(Debug)]
pub enum NetworkTileMsg {
    NetworkUpdate(NetworkInfo),
}

impl SimpleComponent for NetworkTile {
    type Init = ();
    type Input = NetworkTileMsg;
    type Output = TileOutput;
    type Root = gtk::Box;
    type Widgets = ();

    fn init(
        _init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        // initialize the Tile component
        let tile = Tile::builder().launch(Default::default()).detach();

        root.append(tile.widget());

        // start network service worker
        let network_worker =
            NetworkService::builder()
                .detach_worker(())
                .forward(sender.input_sender(), |output| match output {
                    NetworkWorkerOutput::StateChanged(info) => NetworkTileMsg::NetworkUpdate(info),
                });

        let model = NetworkTile {
            _network_info: NetworkInfo::default(),
            tile,
            _worker: network_worker,
        };

        ComponentParts { model, widgets: () }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            NetworkTileMsg::NetworkUpdate(info) => {
                let icon = get_icon(&info);

                self.tile.emit(TileMsg::UpdateData {
                    icon: Some(icon.to_string()),
                    primary: text.map(String::from),
                    secondary: None,
                });
            }
        }
    }

    fn init_root() -> Self::Root {
        gtk::Box::new(gtk::Orientation::Horizontal, 0)
    }
}

fn get_icon(info: &NetworkInfo) -> &str {
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

fn get_secondary_text(info: &NetworkInfo) -> Option<&str> {
    if !info.connected {
        return Some("Disconnected");
    }

    Some(match info.state {
        NetworkState::ConnectedLocal
        | NetworkState::ConnectedGlobal
        | NetworkState::ConnectedSite => return None,
        NetworkState::Unknown => "State unknown",
        NetworkState::Asleep => "Asleep",
        NetworkState::Disconnected => "Disconnected",
        NetworkState::Disconnecting => "Disconnecting",
        NetworkState::Connecting => "Connecting",
    })
}
