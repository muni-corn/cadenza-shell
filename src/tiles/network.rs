use gdk4::Monitor;
use gtk4::prelude::*;
use relm4::prelude::*;

use crate::{
    network::{NETWORK_STATE, NetworkInfo, SpecificNetworkInfo, get_icon, types::State},
    network_menu::NetworkMenu,
    tiles::Attention,
    widgets::{
        menu_window::{MenuWindow, MenuWindowInit, MenuWindowMsg, MenuWindowOutput},
        tile::{Tile, TileInit, TileMsg, TileOutput},
    },
};

#[derive(Debug)]
pub struct NetworkTile {
    current_state: NetworkInfo,
    menu_open: bool,
    menu_window: Controller<MenuWindow>,
}

#[derive(Debug)]
pub enum NetworkTileMsg {
    Update(NetworkInfo),
    ToggleMenu,
    MenuClosed,
}

#[derive(Debug)]
pub struct NetworkTileWidgets {
    tile: Controller<Tile>,
    // kept alive so the menu's component runtime (and its NETWORK_STATE
    // subscription) isn't shut down; dropping a Controller stops its
    // runtime immediately
    _network_menu: Controller<NetworkMenu>,
}

impl SimpleComponent for NetworkTile {
    type Init = Monitor;
    type Input = NetworkTileMsg;
    type Output = TileOutput;
    type Root = gtk::Box;
    type Widgets = NetworkTileWidgets;

    fn init(
        monitor: Self::Init,
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

        // present it in a keyboard-interactive layer-shell window instead of
        // a gtk::Popover, since the bar itself never accepts keyboard focus
        let menu_window = MenuWindow::builder()
            .launch(MenuWindowInit {
                namespace: "network-menu",
                monitor: Some(monitor),
                width: 384,
                content: network_menu.widget().clone().upcast(),
            })
            .forward(sender.input_sender(), |output| match output {
                MenuWindowOutput::Hidden => NetworkTileMsg::MenuClosed,
            });

        // toggle the menu window on click; authoritative for open/closed
        // state so it doesn't depend on the window's own visibility (see
        // MenuWindow's docs on why it doesn't dismiss on focus loss)
        tile.widget().connect_clicked({
            let sender = sender.clone();
            move |_| sender.input(NetworkTileMsg::ToggleMenu)
        });

        root.append(tile.widget());

        ComponentParts {
            model: NetworkTile {
                current_state,
                menu_open: false,
                menu_window,
            },
            widgets: NetworkTileWidgets {
                tile,
                _network_menu: network_menu,
            },
        }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        tracing::debug!("network tile received update: {msg:?}");
        match msg {
            NetworkTileMsg::Update(new_info) => {
                self.current_state = new_info;
            }
            NetworkTileMsg::ToggleMenu => {
                self.menu_open = !self.menu_open;
                self.menu_window.emit(if self.menu_open {
                    MenuWindowMsg::Show
                } else {
                    MenuWindowMsg::Hide
                });
            }
            NetworkTileMsg::MenuClosed => {
                self.menu_open = false;
            }
        }
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
        widgets
            .tile
            .emit(TileMsg::SetAttention(get_attention(&self.current_state)))
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

fn get_attention(info: &NetworkInfo) -> Attention {
    if matches!(
        info.connection_state,
        State::Disconnected | State::Disconnecting | State::Asleep | State::Unknown
    ) {
        Attention::Dim
    } else {
        Attention::Normal
    }
}
