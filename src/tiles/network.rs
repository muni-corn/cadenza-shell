use gtk4::prelude::*;
use relm4::prelude::*;

use crate::{
    network::{NETWORK_STATE, NetworkInfo, SpecificNetworkInfo, get_icon, types::State},
    network_menu::NetworkMenu,
    widgets::tile::{Tile, TileInit, TileMsg, TileOutput},
};

#[derive(Debug)]
pub struct NetworkTile {
    current_state: NetworkInfo,
}

#[derive(Debug)]
pub enum NetworkTileMsg {
    Update(NetworkInfo),
}

#[derive(Debug)]
pub struct NetworkTileWidgets {
    tile: Controller<Tile>,
    _popover: gtk::Popover,
}

impl SimpleComponent for NetworkTile {
    type Init = ();
    type Input = NetworkTileMsg;
    type Output = TileOutput;
    type Root = gtk::Box;
    type Widgets = NetworkTileWidgets;

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
                secondary: get_secondary_text(&current_state),
                tooltip: Some(get_tooltip_text(&current_state)),
                ..Default::default()
            })
            .detach();

        // initialize the network menu component
        let network_menu = NetworkMenu::builder().launch(()).detach();

        // create the popover
        let popover = gtk::Popover::builder()
            .child(network_menu.widget())
            .width_request(384)
            .height_request(256)
            .autohide(true)
            .build();
        popover.set_parent(tile.widget());

        // connect click handler to show popover
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
            model: NetworkTile { current_state },
            widgets: NetworkTileWidgets {
                tile,
                _popover: popover,
            },
        }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        log::debug!("network tile received update: {msg:?}");
        let NetworkTileMsg::Update(new_info) = msg;
        self.current_state = new_info.clone();
    }

    fn update_view(&self, widgets: &mut Self::Widgets, _sender: ComponentSender<Self>) {
        let icon = get_icon(&self.current_state);

        widgets.tile.emit(TileMsg::SetIcon(Some(icon.to_string())));
        widgets.tile.emit(TileMsg::SetPrimary(None));
        widgets.tile.emit(TileMsg::SetSecondary(get_secondary_text(
            &self.current_state,
        )));
        widgets.tile.emit(TileMsg::SetTooltip(Some(get_tooltip_text(
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
