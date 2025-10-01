use gdk4::Monitor;
use gtk4::prelude::*;
use niri_ipc::Workspace;
use relm4::{RelmIterChildrenExt, prelude::*};

use crate::{services::niri::NIRI_STATE, settings::BarConfig};

pub struct NiriInit {
    pub bar_config: BarConfig,
    pub monitor: Monitor,
}

#[derive(Debug)]
pub struct NiriTile {
    pub monitor: Monitor,
}

#[derive(Debug)]
pub struct NiriTileWidgets {
    root: gtk::Box,
    workspaces_container: gtk::Box,
    window_title_label: gtk::Label,
}

#[derive(Debug)]
pub enum NiriMsg {
    Update,
}

impl SimpleComponent for NiriTile {
    type Init = NiriInit;
    type Input = NiriMsg;
    type Output = ();
    type Root = gtk::Box;
    type Widgets = NiriTileWidgets;

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        NIRI_STATE.subscribe(sender.input_sender(), |state| NiriMsg::Update);

        root.set_spacing(init.bar_config.tile_spacing);

        // create workspaces container for visual dots/pill
        let workspaces_container = gtk::Box::new(gtk::Orientation::Horizontal, 8);

        // create window title label
        let window_title_label = gtk::Label::builder()
            .css_classes(["dim"])
            .margin_start(16)
            .max_width_chars(30)
            .ellipsize(pango::EllipsizeMode::End)
            .build();

        root.append(&workspaces_container);
        root.append(&window_title_label);

        let model = NiriTile {
            monitor: init.monitor.clone(),
        };

        // init
        sender.input(NiriMsg::Update);

        ComponentParts {
            model,
            widgets: NiriTileWidgets {
                root,
                workspaces_container,
                window_title_label,
            },
        }
    }

    fn update(&mut self, _msg: Self::Input, _sender: ComponentSender<Self>) {}

    fn update_view(&self, widgets: &mut Self::Widgets, _sender: ComponentSender<Self>) {
        let Some(state) = NIRI_STATE.read().clone() else {
            log::debug!(
                "no niri state for monitor {}, not updating view",
                self.monitor
                    .connector()
                    .unwrap_or_else(|| String::from("(none)").into())
            );
            return;
        };

        widgets.root.set_visible(true);

        state
            .workspaces
            .iter()
            .zip(widgets.workspaces_container.iter_children())
            .for_each(|(ws, ind)| {
                // active workspace - pill shape
                if ws.is_active {
                    ind.add_css_class("active");
                } else {
                    ind.remove_css_class("active");
                }
            });

        // create new indicators for extra workspaces, or remove extras
        let workspace_count = state.workspaces.len();
        let indicator_count = widgets.workspaces_container.iter_children().count();
        if workspace_count > indicator_count {
            // create new indicators
            let extra_workspaces = &state.workspaces[indicator_count..];
            for new_ws in extra_workspaces {
                let indicator = gtk::Box::builder()
                    .css_classes(["workspace"])
                    .height_request(8)
                    .valign(gtk4::Align::Center)
                    .vexpand(false)
                    .build();

                // active workspace - pill shape
                if new_ws.is_active {
                    indicator.add_css_class("active");
                }

                widgets.workspaces_container.append(&indicator);
            }
        } else if indicator_count > workspace_count {
            // remove extra indicators
            for dead_indicator in widgets
                .workspaces_container
                .iter_children()
                .skip(workspace_count)
            {
                widgets.workspaces_container.remove(&dead_indicator);
            }
        }

        // update window title
        widgets
            .window_title_label
            .set_text(&state.focused_window_title);
    }

    fn init_root() -> Self::Root {
        gtk::Box::builder()
            .visible(false)
            .css_classes(["tile"])
            .margin_start(10)
            .build()
    }
}
