mod center;
mod left;
mod right;

use std::sync::{Arc, Mutex};

use gdk4::Monitor;
use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, Layer, LayerShell};
use relm4::prelude::*;
use system_tray::data::BaseMap;

use crate::{
    notifications::center::{NotificationCenter, NotificationCenterMsg},
    settings,
    tray::{TrayEvent, TrayItemOutput},
    widgets::bar::{
        center::CenterGroup,
        left::{LeftGroup, LeftGroupInit},
        right::{RightGroup, RightGroupOutput},
    },
};

#[derive(Debug)]
pub struct Bar {
    monitor: Monitor,
}

#[derive(Debug)]
pub struct BarInit {
    pub monitor: Monitor,
    pub tray_items: Option<Arc<Mutex<BaseMap>>>,
}

pub struct BarWidgets {
    // save Controllers so they aren't dropped
    _left: Controller<LeftGroup>,
    _center: Controller<CenterGroup>,
    _right: Controller<RightGroup>,
    _notification_center: Controller<NotificationCenter>,
}

impl SimpleAsyncComponent for Bar {
    type Init = BarInit;
    type Input = BarMsg;
    type Output = BarOutput;
    type Root = gtk::Window;
    type Widgets = BarWidgets;

    fn init_root() -> Self::Root {
        gtk::Window::builder()
            .title("cadenza-shell")
            .default_height(32)
            .visible(true)
            .build()
    }

    async fn init(
        BarInit {
            monitor,
            tray_items,
        }: Self::Init,
        window: Self::Root,
        sender: AsyncComponentSender<Self>,
    ) -> AsyncComponentParts<Self> {
        let config = settings::get_config();

        // create notification center for this bar/monitor
        let model = Bar { monitor };

        let notification_center = NotificationCenter::builder()
            .launch(model.monitor.clone())
            .detach();

        let left = LeftGroup::builder().launch(LeftGroupInit {
            bar_config: config.bar,
            monitor: model.monitor.clone(),
        });
        let center = CenterGroup::builder().launch(config.bar);
        let right = RightGroup::builder().launch(config.bar).forward(
            notification_center.sender(),
            |right_group_msg| match right_group_msg {
                RightGroupOutput::ToggleNotificationCenter => NotificationCenterMsg::Toggle,
            },
        );

        let bar = gtk::CenterBox::builder()
            .css_classes(["bar"])
            .height_request(config.bar.height)
            .hexpand(true)
            .shrink_center_last(true)
            .start_widget(left.widget())
            .center_widget(center.widget())
            .end_widget(right.widget())
            .build();

        // init layer shell
        if !window.is_layer_window() {
            window.init_layer_shell();
        }

        window.set_monitor(Some(&model.monitor));
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
            _right: right,
            _notification_center: notification_center,
        };

        AsyncComponentParts { model, widgets: () }
    }

    async fn update(&mut self, msg: Self::Input, _sender: AsyncComponentSender<Self>) {}

    fn update_view(&self, _widgets: &mut Self::Widgets, _sender: AsyncComponentSender<Self>) {}
}
