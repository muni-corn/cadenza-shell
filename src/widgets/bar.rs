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
    notifications::center::NotificationCenter,
    settings,
    tray::{TrayEvent, TrayItemOutput},
    widgets::bar::{
        center::CenterGroup,
        left::{LeftGroup, LeftGroupInit},
        right::{RightGroup, RightGroupInit, RightGroupMsg, RightGroupOutput},
    },
};

#[derive(Debug)]
pub struct Bar {
    monitor: Monitor,

    // save Controllers so they aren't dropped
    left: Controller<LeftGroup>,
    center: Controller<CenterGroup>,
    right: Controller<RightGroup>,

    _notification_center: Controller<NotificationCenter>,
}

#[derive(Debug)]
pub struct BarInit {
    pub monitor: Monitor,
    pub tray_items: Option<Arc<Mutex<BaseMap>>>,
}

#[derive(Debug)]
pub enum BarMsg {
    TrayEvent(TrayEvent),
}

#[derive(Debug)]
pub enum BarOutput {
    ToggleNotificationCenter,
    TrayItemOutput(TrayItemOutput),
}

impl SimpleAsyncComponent for Bar {
    type Init = BarInit;
    type Input = BarMsg;
    type Output = BarOutput;
    type Root = gtk::Window;
    type Widgets = ();

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
        let notification_center = NotificationCenter::builder()
            .launch(monitor.clone())
            .detach();

        let model = Bar {
            left: LeftGroup::builder()
                .launch(LeftGroupInit {
                    bar_config: config.bar,
                    monitor: monitor.clone(),
                })
                .detach(),
            center: CenterGroup::builder().launch(config.bar).detach(),
            right: RightGroup::builder()
                .launch(RightGroupInit {
                    bar_config: config.bar,
                    tray_items,
                })
                .forward(sender.output_sender(), |output| match output {
                    RightGroupOutput::ToggleNotificationCenter => {
                        BarOutput::ToggleNotificationCenter
                    }
                    RightGroupOutput::TrayItemOutput(tray_item_output) => {
                        BarOutput::TrayItemOutput(tray_item_output)
                    }
                }),

            _notification_center: notification_center,

            monitor,
        };

        let bar = gtk::CenterBox::builder()
            .css_classes(["bar"])
            .height_request(config.bar.height)
            .hexpand(true)
            .shrink_center_last(true)
            .start_widget(model.left.widget())
            .center_widget(model.center.widget())
            .end_widget(model.right.widget())
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

        AsyncComponentParts { model, widgets: () }
    }

    async fn update(&mut self, msg: Self::Input, _sender: AsyncComponentSender<Self>) {
        match msg {
            // propagate tray update to the right group
            BarMsg::TrayEvent(event) => self.right.emit(RightGroupMsg::TrayEvent(event)),
        }
    }

    fn update_view(&self, _widgets: &mut Self::Widgets, _sender: AsyncComponentSender<Self>) {}
}
