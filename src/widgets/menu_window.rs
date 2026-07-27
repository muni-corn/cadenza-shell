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

/// Emitted whenever the window transitions to hidden, whether requested
/// externally or triggered internally (currently: pressing Escape), so the
/// owning tile can keep its own open/closed tracking in sync without
/// guessing at the window's actual visibility.
#[derive(Debug)]
pub enum MenuWindowOutput {
    Hidden,
}

#[derive(Debug)]
pub struct MenuWindowWidgets {
    window: gtk::Window,
}

impl SimpleComponent for MenuWindow {
    type Init = MenuWindowInit;
    type Input = MenuWindowMsg;
    type Output = MenuWindowOutput;
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
        sender: ComponentSender<Self>,
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

        // dismiss on escape; deliberately not dismissing on focus loss here
        // (e.g. via is-active) since that races against the owning tile's
        // own click-to-toggle handler in a way that isn't reliably testable
        // without a live compositor session
        let key_controller = gtk::EventControllerKey::new();
        let sender_clone = sender.clone();
        key_controller.connect_key_pressed(move |_, key, _, _| {
            if key == gdk4::Key::Escape {
                sender_clone.input(MenuWindowMsg::Hide);
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        window.add_controller(key_controller);

        ComponentParts {
            model: MenuWindow { visible: false },
            widgets: MenuWindowWidgets { window },
        }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            MenuWindowMsg::Show => self.visible = true,
            MenuWindowMsg::Hide => {
                if self.visible {
                    self.visible = false;
                    let _ = sender.output(MenuWindowOutput::Hidden);
                }
            }
        }
    }

    fn update_view(&self, widgets: &mut Self::Widgets, _sender: ComponentSender<Self>) {
        widgets.window.set_visible(self.visible);
    }
}
