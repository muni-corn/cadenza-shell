use gtk4::prelude::*;
use relm4::prelude::*;

use crate::{
    icon_names::{self, *},
    network::{NETWORK_STATE, NetworkInfo, SpecificNetworkInfo, get_icon, types::State},
    utils::icons::{NETWORK_WIFI_ICON_NAMES, percentage_to_icon_from_list},
    widgets::tile::{Tile, TileInit, TileMsg, TileOutput},
};

#[derive(Debug)]
pub struct NetworkTile {
    current_state: NetworkInfo,
}

#[derive(Debug)]
pub enum NetworkTileMsg {
    Click,
    Update(NetworkInfo),
    Nothing,
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
        NETWORK_STATE.subscribe(sender.input_sender(), |state| {
            NetworkTileMsg::Update(state.clone())
        });

        let current_state = NETWORK_STATE.read().clone();

        // initialize the Tile component
        let tile = Tile::builder()
            .launch(TileInit {
                icon_name: Some(get_icon(&current_state).to_string()),
                secondary: get_secondary_text(&current_state).map(String::from),
                tooltip: Some(get_tooltip_text(&current_state)),
                ..Default::default()
            })
            .forward(sender.input_sender(), |output| match output {
                TileOutput::Clicked => NetworkTileMsg::Click,
                _ => NetworkTileMsg::Nothing,
            });

        root.append(tile.widget());

        ComponentParts {
            model: NetworkTile { current_state },
            widgets: tile,
        }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        log::debug!("network tile received update: {msg:?}");
        if let NetworkTileMsg::Update(new_info) = msg {
            self.current_state = new_info;
        }
    }

    fn update_view(&self, tile: &mut Self::Widgets, _sender: ComponentSender<Self>) {
        let icon = get_icon(&self.current_state);

        tile.emit(TileMsg::SetIcon(Some(icon.to_string())));
        tile.emit(TileMsg::SetPrimary(None));
        tile.emit(TileMsg::SetSecondary(
            get_secondary_text(&self.current_state).map(String::from),
        ));
        tile.emit(TileMsg::SetTooltip(Some(get_tooltip_text(
            &self.current_state,
        ))));
    }

    fn init_root() -> Self::Root {
        gtk::Box::builder().build()
    }
}

fn get_secondary_text(info: &NetworkInfo) -> Option<String> {
    Some(match info.connection_state {
        State::ConnectedGlobal => return None,
        c => c.to_string(),
    })
}

fn get_tooltip_text(info: &NetworkInfo) -> String {
    // get the connection state text
    let state_text = info.connection_state.to_string();

    // add specific network info if available
    match &info.specific_info {
        Some(SpecificNetworkInfo::WiFi { wifi_ssid, .. }) => {
            format!("{}\n{}", state_text, wifi_ssid)
        }
        Some(SpecificNetworkInfo::Wired) => format!("{}\nWired connection", state_text),
        None => state_text,
    }
}
