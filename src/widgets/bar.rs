use gdk4::Monitor;
use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, Layer, LayerShell};
use relm4::prelude::*;

use crate::{settings, tiles::clock::ClockTile};

#[derive(Debug)]
pub struct Bar {
    _monitor: Monitor,
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
        let model = Bar { _monitor: monitor };

        // init layer shell
        if !window.is_layer_window() {
            window.init_layer_shell();
        }

        window.set_namespace(Some("bar"));

        window.set_layer(Layer::Top);

        // Use configuration for bar height
        let config = settings::get_config();
        window.set_exclusive_zone(config.bar.height);

        window.set_anchor(Edge::Top, true);
        window.set_anchor(Edge::Left, true);
        window.set_anchor(Edge::Right, true);

        let config = settings::get_config();

        let bar = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .css_classes(["bar"])
            .height_request(config.bar.height)
            .hexpand(true)
            .spacing(config.bar.spacing)
            .margin_start(config.bar.margin)
            .margin_end(config.bar.margin)
            .build();

        window.set_child(Some(&bar));

        // Left section - workspaces and focused window
        let left = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .css_classes(["bar-left"])
            .build();

        // Center section - spacer and clock
        let center = gtk::Box::builder()
            .hexpand(true)
            .halign(gtk::Align::Center)
            .orientation(gtk::Orientation::Horizontal)
            .build();

        let clock = ClockTile::builder().launch(()).detach();

        center.append(clock.widget());

        // Right section - system tiles
        let right = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
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
