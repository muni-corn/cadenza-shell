mod center;
mod left;
mod right;

use gdk4::Monitor;
use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, Layer, LayerShell};
use relm4::prelude::*;

use crate::{
    settings,
    widgets::bar::{center::CenterGroup, left::LeftGroup, right::RightGroup},
};

#[derive(Debug)]
pub struct Bar {
    monitor: Monitor,
}

#[derive(Debug)]
pub enum BarMsg {}

#[derive(Debug)]
pub struct BarWidgets {
    // save Controllers so they aren't dropped
    _left: Controller<LeftGroup>,
    _center: Controller<CenterGroup>,
    _right: Controller<RightGroup>,
}

impl SimpleComponent for Bar {
    type Init = Monitor;
    type Input = BarMsg;
    type Output = ();
    type Root = gtk::Window;
    type Widgets = BarWidgets;

    fn init_root() -> Self::Root {
        gtk::Window::builder()
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
        let model = Bar { monitor };

        let config = settings::get_config();

        let left = LeftGroup::builder().launch((model.monitor.clone(), config.bar));
        let center = CenterGroup::builder().launch(config.bar);
        let right = RightGroup::builder().launch(config.bar);

        let bar = gtk::CenterBox::builder()
            .css_classes(["bar"])
            .height_request(config.bar.height)
            .hexpand(true)
            .shrink_center_last(true)
            .margin_start(config.bar.margin)
            .margin_end(config.bar.margin)
            .start_widget(left.widget())
            .center_widget(center.widget())
            .end_widget(right.widget())
            .build();

        // init layer shell
        if !window.is_layer_window() {
            window.init_layer_shell();
        }

        window.set_namespace(Some("bar"));
        window.set_layer(Layer::Top);
        window.set_exclusive_zone(config.bar.height);
        window.set_anchor(Edge::Top, true);
        window.set_anchor(Edge::Left, true);
        window.set_anchor(Edge::Right, true);
        window.set_child(Some(&bar));

        let widgets = BarWidgets {
            _left: left.detach(),
            _center: center.detach(),
            _right: right.detach(),
        };

        ComponentParts { model, widgets }
    }

    fn update(&mut self, _msg: Self::Input, _sender: ComponentSender<Self>) {}

    fn update_view(&self, _widgets: &mut Self::Widgets, _sender: ComponentSender<Self>) {}
}
