use gdk4::Monitor;
use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, Layer, LayerShell};
use relm4::prelude::*;

use crate::{settings, tiles::clock::ClockTile};

#[derive(Debug)]
pub struct Bar {
    monitor: Monitor,
}

#[derive(Debug)]
pub enum BarMsg {}

#[derive(Debug)]
pub struct BarWidgets {
    _clock: Controller<ClockTile>, // saved so the Controller isn't dropped
}

impl SimpleComponent for Bar {
    type Init = Monitor;
    type Input = BarMsg;
    type Output = ();
    type Root = gtk::ApplicationWindow;
    type Widgets = BarWidgets;

    fn init_root() -> Self::Root {
        gtk::ApplicationWindow::builder()
            .title("muse-shell")
            .default_height(32)
            .visible(true)
            .build()
    }

    fn init(
        monitor: Self::Init,
        window: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        log::debug!("Initializing layer shell for bar window");

        let model = Bar { monitor };

        // init layer shell
        if !window.is_layer_window() {
            window.init_layer_shell();
            log::debug!("layer shell initialized: {}", window.is_layer_window());
        } else {
            log::debug!("window already is a layer window");
        }

        window.set_layer(Layer::Top);
        log::debug!("set layer to top");

        // Use configuration for bar height
        let config = settings::get_config();
        window.set_exclusive_zone(config.bar.height);
        log::debug!("set exclusive zone to: {}", config.bar.height);

        window.set_anchor(Edge::Top, true);
        window.set_anchor(Edge::Left, true);
        window.set_anchor(Edge::Right, true);
        log::debug!("set anchors: Top, Left, Right");

        let config = settings::get_config();

        let bar = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .css_classes(["bar"])
            .height_request(config.bar.height)
            .hexpand(true)
            .spacing(config.bar.spacing)
            .margin_start(config.bar.margin)
            .margin_end(config.bar.margin)
            .build();

        window.set_child(Some(&bar));

        // Left section - workspaces and focused window
        let left = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(8)
            .css_classes(["bar-left"])
            .build();

        // Center section - spacer and clock
        let center = gtk4::Box::builder()
            .hexpand(true)
            .halign(gtk4::Align::Center)
            .orientation(gtk4::Orientation::Horizontal)
            .build();

        let clock = ClockTile::builder().launch(()).detach();

        center.append(clock.widget());

        // Right section - system tiles
        let right = gtk4::Box::builder()
            .orientation(gtk4::Orientation::Horizontal)
            .spacing(8)
            .css_classes(["bar-right"])
            .build();

        bar.append(&left);
        bar.append(&center);
        bar.append(&right);

        let widgets = BarWidgets { _clock: clock };

        ComponentParts { model, widgets }
    }

    fn update(&mut self, _msg: Self::Input, _sender: ComponentSender<Self>) {}

    fn update_view(&self, _widgets: &mut Self::Widgets, _sender: ComponentSender<Self>) {}
}
