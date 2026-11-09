mod tiles;

use std::sync::Arc;

use gdk4::Monitor;
use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, Layer, LayerShell};
use relm4::prelude::*;

use crate::{
    notifications::panel::{ActionPanel, ActionPanelMsg},
    settings,
    widgets::{bar::tiles::BarTiles, tray_item::TrayClient},
};

#[derive(Debug)]
pub struct Bar {
    monitor: Monitor,
    /// The root layer-shell window. Stored in the model (rather than Widgets)
    /// so we can explicitly close it in shutdown(), which prevents the
    /// compositor from migrating the layer surface to another output.
    window: gtk::Window,

    _tiles: BarTiles,

    /// This bar's own action panel, toggled by its bell button.
    action_panel: Controller<ActionPanel>,
}

#[derive(Debug)]
pub struct BarInit {
    pub monitor: Monitor,
    pub tray_client: Option<Arc<TrayClient>>,
}

#[derive(Debug)]
pub enum BarMsg {
    ToggleActionPanel,
}

impl SimpleComponent for Bar {
    type Init = BarInit;
    type Input = BarMsg;
    type Output = ();
    type Root = gtk::Window;
    type Widgets = ();

    fn init_root() -> Self::Root {
        gtk::Window::builder()
            .title("cadenza-shell")
            .default_height(32)
            // stays unrealized until layer shell is configured in init
            .visible(false)
            .build()
    }

    fn init(
        BarInit {
            monitor,
            tray_client,
        }: Self::Init,
        window: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let config = settings::get_config();

        // each bar owns the action panel for its own monitor
        let action_panel = ActionPanel::builder().launch(monitor.clone()).detach();

        let (tiles, groups) =
            BarTiles::launch(config.bar, &monitor, tray_client, sender.input_sender());

        let model = Bar {
            _tiles: tiles,
            action_panel,
            window: window.clone(),
            monitor,
        };

        let bar = gtk::CenterBox::builder()
            .css_classes(["bar"])
            .height_request(config.bar.height)
            .hexpand(true)
            .shrink_center_last(true)
            .start_widget(&groups.start)
            .center_widget(&groups.center)
            .end_widget(&groups.end)
            .build();

        // gtk4-layer-shell must be initialized before the window is realized
        window.init_layer_shell();

        window.set_monitor(Some(&model.monitor));
        window.set_namespace(Some("bar"));
        window.set_layer(Layer::Top);
        window.set_exclusive_zone(config.bar.height);
        window.set_anchor(Edge::Top, true);
        window.set_anchor(Edge::Left, true);
        window.set_anchor(Edge::Right, true);
        window.set_child(Some(&bar));

        window.present();

        ComponentParts { model, widgets: () }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            BarMsg::ToggleActionPanel => self.action_panel.emit(ActionPanelMsg::Toggle),
        }
    }

    fn shutdown(&mut self, _widgets: &mut Self::Widgets, _output: relm4::Sender<Self::Output>) {
        // explicitly close the layer-shell window so the compositor destroys
        // the surface immediately; without this, GTK may finalize the GtkWindow
        // while the layer surface is still alive, which causes wlr-layer-shell
        // compositors (including Niri) to move the surface to another output
        tracing::debug!(
            "shutting down bar for monitor: {:?}",
            self.monitor.connector()
        );
        self.window.close();
    }
}
