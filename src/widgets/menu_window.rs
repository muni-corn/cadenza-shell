// temporary until wired up by the network and bluetooth tiles
#![allow(dead_code)]

use gdk4::Monitor;
use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use relm4::prelude::*;

use crate::settings;

/// A small dropdown-style layer-shell window for quick-settings-like menus
/// (e.g. the wifi and bluetooth dropdowns).
///
/// The bar itself never accepts keyboard focus, so a `gtk::Popover` parented
/// to it can never receive keystrokes - there is no way to type a wifi
/// password or a bluetooth PIN into one. This is a separate `Top`-layer
/// surface with `KeyboardMode::OnDemand`, which the compositor grants
/// keyboard focus to only while it's visible.
#[derive(Debug)]
pub struct MenuWindow {
    visible: bool,
}

pub struct MenuWindowInit {
    /// Layer-shell namespace, useful for distinguishing this window in
    /// compositor window-rule configuration (e.g. `"network-menu"`).
    pub namespace: &'static str,
    /// Monitor to pin this window to, matching the bar/tile that owns it.
    /// `None` lets the compositor choose (e.g. the currently focused
    /// output).
    pub monitor: Option<Monitor>,
    /// Requested content width; height is left to the content to size
    /// itself, since layer-shell windows without a bottom anchor size to
    /// their child's natural height.
    pub width: i32,
    /// The pre-built content widget to embed (typically a menu component's
    /// root widget).
    pub content: gtk::Widget,
}

#[derive(Debug)]
pub enum MenuWindowMsg {
    Show,
    Hide,
}

pub struct MenuWindowWidgets {
    window: gtk::Window,
}

impl SimpleComponent for MenuWindow {
    type Init = MenuWindowInit;
    type Input = MenuWindowMsg;
    type Output = ();
    type Root = gtk::Window;
    type Widgets = MenuWindowWidgets;

    fn init_root() -> Self::Root {
        gtk::Window::builder()
            .title("cadenza menu")
            .visible(false)
            .build()
    }

    fn init(
        init: Self::Init,
        window: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        // sit just below the bar, regardless of the configured bar height
        let top_margin = settings::get_config().bar.height + 4;

        window.init_layer_shell();
        if let Some(monitor) = &init.monitor {
            window.set_monitor(Some(monitor));
        }
        window.set_namespace(Some(init.namespace));
        window.set_layer(Layer::Top);
        window.set_keyboard_mode(KeyboardMode::OnDemand);
        window.set_anchor(Edge::Top, true);
        window.set_anchor(Edge::Right, true);
        window.set_margin(Edge::Top, top_margin);
        window.set_margin(Edge::Right, 8);
        window.set_width_request(init.width);
        window.set_child(Some(&init.content));

        ComponentParts {
            model: MenuWindow { visible: false },
            widgets: MenuWindowWidgets { window },
        }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            MenuWindowMsg::Show => self.visible = true,
            MenuWindowMsg::Hide => self.visible = false,
        }
    }

    fn update_view(&self, widgets: &mut Self::Widgets, _sender: ComponentSender<Self>) {
        widgets.window.set_visible(self.visible);
    }
}
