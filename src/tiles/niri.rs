use gdk4::Monitor;
use gtk4::prelude::*;
use relm4::{RelmIterChildrenExt, Worker, prelude::*};

use crate::{
    services::niri::{NiriService, NiriUpdate},
    settings::BarConfig,
};

pub struct NiriInit {
    pub bar_config: BarConfig,
    pub monitor: Monitor,
}

#[derive(Debug)]
pub struct NiriTile {
    available: bool,
    workspaces: Vec<niri_ipc::Workspace>,
    focused_window_title: String,

    _service: Controller<NiriService>,
}

#[derive(Debug)]
pub struct NiriTileWidgets {
    root: gtk::Box,
    workspaces_container: gtk::Box,
    window_title_label: gtk::Label,
}

#[derive(Debug)]
pub enum NiriMsg {
    ServiceUpdate(<NiriService as Worker>::Output),
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
        let _service = NiriService::builder()
            .launch(())
            .forward(sender.input_sender(), NiriMsg::ServiceUpdate);

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
            available: false,
            workspaces: Vec::new(),
            focused_window_title: String::new(),
            _service,
        };

        ComponentParts {
            model,
            widgets: NiriTileWidgets {
                root,
                workspaces_container,
                window_title_label,
            },
        }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            NiriMsg::ServiceUpdate(m) => match m {
                NiriUpdate::State {
                    mut workspaces,
                    focused_window_title,
                } => {
                    workspaces.sort_by_key(|ws| ws.idx);
                    self.workspaces = workspaces;
                    self.focused_window_title = focused_window_title;
                    self.available = true;
                }
                NiriUpdate::Unavailable => {
                    self.available = false;
                }
            },
        }
    }

    fn update_view(&self, widgets: &mut Self::Widgets, _sender: ComponentSender<Self>) {
        self.workspaces
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
        let workspace_count = self.workspaces.len();
        let indicator_count = widgets.workspaces_container.iter_children().count();
        if workspace_count > indicator_count {
            // create new indicators
            let extra_workspaces = &self.workspaces[indicator_count..];
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
            .set_text(&self.focused_window_title);

        // show if available
        widgets.root.set_visible(self.available);
    }

    fn init_root() -> Self::Root {
        gtk::Box::builder()
            .visible(false)
            .css_classes(["tile"])
            .margin_start(10)
            .build()
    }
}
