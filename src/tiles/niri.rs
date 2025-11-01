use std::collections::HashSet;

use gdk4::Monitor;
use gtk4::prelude::*;
use niri_ipc::Workspace;
use relm4::prelude::*;

use crate::{niri::NIRI_STATE, settings::BarConfig};

pub struct NiriInit {
    pub bar_config: BarConfig,
    pub monitor: Monitor,
}

#[derive(Debug)]
pub struct NiriTile {
    pub monitor_connector_name: Option<String>,

    workspaces: FactoryVecDeque<NiriWorkspaceIndicator>,
}

#[derive(Debug)]
pub struct NiriTileWidgets {
    root: gtk::Box,
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
        NIRI_STATE.subscribe(sender.input_sender(), |_| NiriMsg::Update);

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
            monitor_connector_name: init.monitor.connector().map(String::from),
            workspaces: FactoryVecDeque::builder()
                .launch(workspaces_container)
                .detach(),
        };

        // init
        sender.input(NiriMsg::Update);

        ComponentParts {
            model,
            widgets: NiriTileWidgets {
                root,
                window_title_label,
            },
        }
    }

    fn update(&mut self, _msg: Self::Input, _sender: ComponentSender<Self>) {
        let Some(state) = NIRI_STATE.read().clone() else {
            log::debug!("no niri state, not updating niri tile",);
            return;
        };

        let monitor_workspaces: Vec<&Workspace> = state
            .workspaces
            .iter()
            .filter(|w| w.output == self.monitor_connector_name)
            .collect();

        let new_ids: HashSet<u64> = monitor_workspaces.iter().map(|w| w.id).collect();

        let mut guard = self.workspaces.guard();

        let to_remove: Vec<usize> = guard
            .iter()
            .enumerate()
            .filter_map(|(i, ws)| {
                if !new_ids.contains(&ws.inner.id) {
                    Some(i)
                } else {
                    None
                }
            })
            .collect();

        for index in to_remove.into_iter().rev() {
            guard.remove(index);
        }

        for ws in monitor_workspaces {
            if let Some(index) = guard.iter().position(|existing| existing.inner.id == ws.id) {
                guard.send(index, NiriWorkspaceMsg::Update(ws.to_owned()));
            } else {
                guard.push_back(ws.to_owned());
            }
        }
    }

    fn update_view(&self, widgets: &mut Self::Widgets, _sender: ComponentSender<Self>) {
        let Some(state) = NIRI_STATE.read().clone() else {
            log::debug!("no niri state, not updating niri tile view",);
            return;
        };

        widgets.root.set_visible(true);

        // update window title
        if self.monitor_connector_name == Some(state.focused_output) {
            widgets.window_title_label.set_visible(true);
            widgets
                .window_title_label
                .set_text(&state.focused_window_title);
        } else {
            widgets.window_title_label.set_visible(false);
        };
    }

    fn init_root() -> Self::Root {
        gtk::Box::builder()
            .visible(false)
            .css_classes(["tile"])
            .margin_start(10)
            .build()
    }
}

/// A small widget for a single niri workspace indicator.
#[derive(Debug)]
pub struct NiriWorkspaceIndicator {
    inner: niri_ipc::Workspace,
}

#[derive(Debug)]
pub enum NiriWorkspaceMsg {
    Update(niri_ipc::Workspace),
}

impl FactoryComponent for NiriWorkspaceIndicator {
    type CommandOutput = ();
    type Index = DynamicIndex;
    type Init = niri_ipc::Workspace;
    type Input = NiriWorkspaceMsg;
    type Output = ();
    type ParentWidget = gtk::Box;
    type Root = gtk::Box;
    type Widgets = gtk::Box;

    fn init_model(inner: Self::Init, _index: &Self::Index, _sender: FactorySender<Self>) -> Self {
        Self { inner }
    }

    fn init_root(&self) -> Self::Root {
        gtk::Box::builder()
            .css_classes(["workspace"])
            .height_request(8)
            .valign(gtk4::Align::Center)
            .vexpand(false)
            .build()
    }

    fn init_widgets(
        &mut self,
        _index: &Self::Index,
        root: Self::Root,
        _returned_widget: &<Self::ParentWidget as relm4::factory::FactoryView>::ReturnedWidget,
        _sender: FactorySender<Self>,
    ) -> Self::Widgets {
        if self.inner.is_active {
            root.add_css_class("active");
        }
        root
    }

    fn update(&mut self, msg: Self::Input, _sender: FactorySender<Self>) {
        match msg {
            NiriWorkspaceMsg::Update(ws) => {
                self.inner = ws;
            }
        }
    }

    fn update_view(&self, widgets: &mut Self::Widgets, _sender: FactorySender<Self>) {
        if self.inner.is_active {
            widgets.add_css_class("active");
        } else {
            widgets.remove_css_class("active");
        }
    }
}
