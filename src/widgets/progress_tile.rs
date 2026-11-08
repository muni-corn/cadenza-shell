use gtk4::prelude::*;
use relm4::prelude::*;

use crate::{settings, tiles::Attention};

#[derive(Debug)]
pub struct ProgressTile {
    icon: Option<String>,
    progress: f64,
    attention: Attention,
    active: bool,
    /// Bumped on every activation so stale fade timers can be ignored.
    fade_generation: u64,
    /// Whether a progress value has arrived since init; the first one is
    /// the initial state, not a change worth highlighting.
    received_progress: bool,
}

#[derive(Debug)]
pub enum ProgressTileMsg {
    /// Sets the icon of the ProgressTile.
    SetIcon(Option<String>),

    /// Sets the fraction of the ProgressTile, expanding it with an animation to
    /// show its fraction.
    SetProgress(f64),
}

#[derive(Debug)]
pub struct ProgressTileWidgets {
    icon: gtk::Image,
    progress_bar: gtk::ProgressBar,
}

pub struct ProgressTileInit {
    pub icon_name: Option<String>,
    pub progress: f64,
    pub attention: Attention,
}

impl Default for ProgressTileInit {
    fn default() -> Self {
        Self {
            icon_name: None,
            progress: 0.0,
            attention: Attention::Normal,
        }
    }
}

/// How long a tile stays highlighted after its progress changes.
const FADE_DELAY: std::time::Duration = std::time::Duration::from_secs(3);

impl Component for ProgressTile {
    type CommandOutput = u64;
    type Init = ProgressTileInit;
    type Input = ProgressTileMsg;
    type Output = ();
    type Root = gtk::Button;
    type Widgets = ProgressTileWidgets;

    fn init(
        init: Self::Init,
        root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = ProgressTile {
            icon: init.icon_name,
            progress: init.progress,
            attention: init.attention,
            active: false,
            fade_generation: 0,
            received_progress: false,
        };

        // create container
        let container = gtk::Box::new(gtk::Orientation::Horizontal, 8);

        // create icon
        let icon = gtk::Image::builder()
            .css_classes(model.attention.with_classes(&["icon"]))
            .pixel_size(settings::get_config().tiles.icon_size)
            .width_request(settings::get_config().tiles.icon_size)
            .build();

        // create progress bar
        let progress_bar = gtk::ProgressBar::builder()
            .css_classes(model.attention.with_classes(&[]))
            .fraction(model.progress)
            .valign(gtk::Align::Center)
            .build();

        // add widgets to container
        container.append(&icon);
        container.append(&progress_bar);

        root.set_child(Some(&container));

        // set initial values
        if let Some(icon_name) = &model.icon {
            icon.set_icon_name(Some(icon_name));
            icon.set_visible(true);
        } else {
            icon.set_visible(false);
        }

        let widgets = ProgressTileWidgets { icon, progress_bar };

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>, _root: &Self::Root) {
        match msg {
            ProgressTileMsg::SetIcon(icon) => {
                self.icon = icon;
            }
            ProgressTileMsg::SetProgress(progress) => {
                // only highlight real changes, not repeats of the same value
                // from unrelated updates
                let changed = (progress - self.progress).abs() > f64::EPSILON;
                self.progress = progress;
                if changed && self.received_progress {
                    self.activate(&sender);
                }
                self.received_progress = true;
            }
        }
    }

    fn update_cmd(
        &mut self,
        generation: Self::CommandOutput,
        _sender: ComponentSender<Self>,
        _root: &Self::Root,
    ) {
        // a newer activation restarted the timer, so this one is stale
        if generation == self.fade_generation {
            self.active = false;
        }
    }

    fn update_view(&self, widgets: &mut Self::Widgets, _sender: ComponentSender<Self>) {
        // update attention CSS classes
        widgets
            .icon
            .set_css_classes(&self.attention.with_classes(&["icon"]));
        widgets
            .progress_bar
            .set_css_classes(&self.attention.with_classes(&[]));

        if self.active {
            widgets.icon.add_css_class("active");
            widgets.progress_bar.add_css_class("active");
        }

        // update icon
        if let Some(icon_name) = &self.icon {
            widgets.icon.set_icon_name(Some(icon_name));
            widgets.icon.set_visible(true);
        } else {
            widgets.icon.set_visible(false);
        }

        // update progress bar
        widgets.progress_bar.set_fraction(self.progress);
    }

    fn init_root() -> Self::Root {
        gtk::Button::builder().css_classes(["tile"]).build()
    }
}

impl ProgressTile {
    fn activate(&mut self, sender: &ComponentSender<Self>) {
        self.active = true;
        self.fade_generation = self.fade_generation.wrapping_add(1);

        // schedule fade back to dim; relm4 cancels the command if the tile
        // shuts down first
        let generation = self.fade_generation;
        sender.oneshot_command(async move {
            tokio::time::sleep(FADE_DELAY).await;
            generation
        });
    }
}
