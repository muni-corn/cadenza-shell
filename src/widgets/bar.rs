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
    notifications::panel::ActionPanel,
    settings,
    widgets::{
        bar::{
            center::CenterGroup,
            left::{LeftGroup, LeftGroupInit},
            right::{RightGroup, RightGroupInit, RightGroupMsg, RightGroupOutput},
        },
        tray_item::{TrayEvent, TrayItemOutput},
    },
};

#[derive(Debug)]
pub struct Bar {
    monitor: Monitor,
    /// The root layer-shell window. Stored in the model (rather than Widgets)
    /// so we can explicitly close it in shutdown(), which prevents the
    /// compositor from migrating the layer surface to another output.
    window: gtk::Window,

    // save Controllers so they aren't dropped
    left: Controller<LeftGroup>,
    center: Controller<CenterGroup>,
    right: Controller<RightGroup>,

    notification_center: Controller<ActionPanel>,
}

#[derive(Debug)]
pub struct BarInit {
    pub monitor: Monitor,
    pub tray_items: Option<Arc<Mutex<BaseMap>>>,
}

#[derive(Debug)]
pub enum BarMsg {
    TrayEvent(TrayEvent),
    ToggleNotificationCenter,
}

#[derive(Debug)]
pub enum BarOutput {
    ToggleNotificationCenter,
    TrayItemOutput(TrayItemOutput),
    /// Emitted when the bar's monitor becomes invalid so the app can remove
    /// and drop the bar. Carries the connector name used as the map key.
    MonitorInvalidated(String),
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
        let notification_center = ActionPanel::builder().launch(monitor.clone()).detach();

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

            notification_center,

            window: window.clone(),
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

        // listen for the monitor becoming invalid (e.g. display unplugged); GDK
        // emits this signal before the compositor destroys the layer-shell
        // surface, giving us the chance to drop the bar cleanly and avoid the
        // surface migrating to another output
        let output_sender = sender.output_sender().clone();
        let connector = model
            .monitor
            .connector()
            .map(|c| c.to_string())
            .unwrap_or_default();
        model.monitor.connect_invalidate(move |_| {
            log::info!(
                "monitor invalidated, notifying app to remove bar for: {}",
                connector
            );
            if output_sender
                .send(BarOutput::MonitorInvalidated(connector.clone()))
                .is_err()
            {
                log::error!("failed to send MonitorInvalidated: receiver already dropped");
            }
        });

        AsyncComponentParts { model, widgets: () }
    }

    async fn update(&mut self, msg: Self::Input, _sender: AsyncComponentSender<Self>) {
        match msg {
            // propagate tray update to the right group
            BarMsg::TrayEvent(event) => self.right.emit(RightGroupMsg::TrayEvent(event)),
            BarMsg::ToggleNotificationCenter => {
                use crate::notifications::panel::ActionPanelMsg;
                self.notification_center.emit(ActionPanelMsg::Toggle);
            }
        }
    }

    fn update_view(&self, _widgets: &mut Self::Widgets, _sender: AsyncComponentSender<Self>) {}

    fn shutdown(&mut self, _widgets: &mut Self::Widgets, _output: relm4::Sender<Self::Output>) {
        // explicitly close the layer-shell window so the compositor destroys the
        // surface immediately; without this, GTK may finalize the GtkWindow while
        // the layer surface is still alive, which causes wlr-layer-shell
        // compositors (including Niri) to move the surface to another output
        log::debug!(
            "shutting down bar for monitor: {:?}",
            self.monitor.connector()
        );
        self.window.close();
    }
}
