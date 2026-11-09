use std::sync::atomic::{AtomicUsize, Ordering};

use gdk4::Monitor;
use gtk4::prelude::*;
use relm4::prelude::*;

use crate::{
    bluetooth::{BLUETOOTH_STATE, BluetoothState, PAIRING_PROMPT, get_icon},
    bluetooth_menu::{BluetoothMenu, BluetoothMenuMsg},
    niri::NIRI_STATE,
    widgets::{
        menu_window::{MenuWindow, MenuWindowInit, MenuWindowMsg, MenuWindowOutput},
        tile::{Tile, TileOutput, TileState, state_tile},
    },
};

/// How many bluetooth menus are open, across every monitor's bar.
static OPEN_MENUS: AtomicUsize = AtomicUsize::new(0);

/// The bluetooth tile and the menu it opens.
#[derive(Debug)]
pub struct BluetoothTile {
    /// The connector of the monitor this tile's bar is on, e.g. "DP-1".
    connector: Option<String>,
    menu_window: Controller<MenuWindow>,
    bluetooth_menu: Controller<BluetoothMenu>,
    menu_shown: bool,
    prompt_pending: bool,
}

#[derive(Debug)]
pub struct BluetoothWidgets {
    _tile: Controller<Tile>,
}

#[derive(Debug)]
pub enum BluetoothTileMsg {
    TileClicked,
    MenuShown(bool),
    /// A pairing prompt appeared or went away.
    PromptPending(bool),
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
        let connector = monitor.connector().map(String::from);
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

        PAIRING_PROMPT.subscribe(sender.input_sender(), |prompt| {
            BluetoothTileMsg::PromptPending(prompt.is_some())
        });

        root.append(tile.widget());

        ComponentParts {
            model: Self {
                connector,
                menu_window,
                bluetooth_menu,
                menu_shown: false,
                prompt_pending: PAIRING_PROMPT.read().is_some(),
            },
            widgets: BluetoothWidgets { _tile: tile },
        }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            BluetoothTileMsg::TileClicked => self.menu_window.emit(MenuWindowMsg::Toggle),
            BluetoothTileMsg::MenuShown(shown) => {
                self.set_menu_shown(shown);
                self.bluetooth_menu.emit(BluetoothMenuMsg::SetOpen(shown));
            }
            BluetoothTileMsg::PromptPending(pending) => {
                let appeared = pending && !self.prompt_pending;
                self.prompt_pending = pending;

                let focused_output = NIRI_STATE
                    .read()
                    .as_ref()
                    .map(|state| state.focused_output.clone());
                if appeared
                    && should_open_for_prompt(
                        self.connector.as_deref(),
                        focused_output.as_deref(),
                        OPEN_MENUS.load(Ordering::Relaxed) > 0,
                    )
                {
                    self.menu_window.emit(MenuWindowMsg::Show);
                }
            }
        }
    }

    fn shutdown(&mut self, _widgets: &mut Self::Widgets, _output: relm4::Sender<Self::Output>) {
        // an unplugged monitor's menu never reports being hidden
        self.set_menu_shown(false);
    }

    fn init_root() -> Self::Root {
        gtk::Box::new(gtk::Orientation::Horizontal, 0)
    }
}

impl BluetoothTile {
    /// Tracks whether this tile's menu is open, keeping the global count of
    /// open menus in step.
    fn set_menu_shown(&mut self, shown: bool) {
        if shown == self.menu_shown {
            return;
        }
        self.menu_shown = shown;
        if shown {
            OPEN_MENUS.fetch_add(1, Ordering::Relaxed);
        } else {
            OPEN_MENUS.fetch_sub(1, Ordering::Relaxed);
        }
    }
}

/// Whether the menu on the monitor with `connector` should open for a new
/// pairing prompt.
///
/// Incoming requests time out unseen if no menu is open, so the menu opens
/// on the monitor the user is looking at; if a menu is already open, the
/// prompt is shown there instead.
fn should_open_for_prompt(
    connector: Option<&str>,
    focused_output: Option<&str>,
    any_menu_open: bool,
) -> bool {
    !any_menu_open && connector.is_some() && connector == focused_output
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_only_on_the_focused_monitor() {
        assert!(should_open_for_prompt(Some("DP-1"), Some("DP-1"), false));
        assert!(!should_open_for_prompt(
            Some("DP-1"),
            Some("HDMI-A-1"),
            false
        ));
    }

    #[test]
    fn stays_closed_when_a_menu_is_already_open_or_focus_is_unknown() {
        assert!(!should_open_for_prompt(Some("DP-1"), Some("DP-1"), true));
        assert!(!should_open_for_prompt(Some("DP-1"), None, false));
        assert!(!should_open_for_prompt(None, None, false));
    }
}
