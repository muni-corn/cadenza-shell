use gtk4::prelude::*;
use relm4::{SharedState, prelude::*};

use crate::{settings, tiles::Attention};

/// What a progress tile shows.
#[derive(Clone, Debug, PartialEq)]
pub struct ProgressState {
    pub icon: String,
    /// How full the bar is, from 0 to 1.
    pub progress: f64,
}

/// A dimmed bar button with an icon and a progress bar that lights up for a
/// few seconds whenever its value changes, e.g. volume or brightness.
#[derive(Debug)]
pub struct ProgressTile {
    /// `None` hides the tile.
    state: Option<ProgressState>,
    active: bool,
    /// Bumped on every activation so stale fade timers can be ignored.
    fade_generation: u64,
}

#[derive(Debug)]
pub struct ProgressTileWidgets {
    root: gtk::Button,
    icon: gtk::Image,
    progress_bar: gtk::ProgressBar,
}

/// How long a tile stays highlighted after its progress changes.
const FADE_DELAY: std::time::Duration = std::time::Duration::from_secs(3);

/// Progress tiles stay dim at rest so they don't compete with other tiles.
const ATTENTION: Attention = Attention::Dim;

impl Component for ProgressTile {
    type CommandOutput = u64;
    /// The initial state; `None` starts the tile hidden.
    type Init = Option<ProgressState>;
    /// Replaces what the tile shows; `None` hides it.
    type Input = Option<ProgressState>;
    type Output = ();
    type Root = gtk::Button;
    type Widgets = ProgressTileWidgets;

    fn init(
        state: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = ProgressTile {
            state,
            active: false,
            fade_generation: 0,
        };

        let container = gtk::Box::new(gtk::Orientation::Horizontal, 8);

        let icon_size = settings::get_config().tiles.icon_size;
        let icon = gtk::Image::builder()
            .css_classes(ATTENTION.with_classes(&["icon"]))
            .pixel_size(icon_size)
            .width_request(icon_size)
            .build();

        let progress_bar = gtk::ProgressBar::builder()
            .css_classes(ATTENTION.with_classes(&[]))
            .valign(gtk::Align::Center)
            .build();

        container.append(&icon);
        container.append(&progress_bar);
        root.set_child(Some(&container));

        let mut widgets = ProgressTileWidgets {
            root,
            icon,
            progress_bar,
        };
        Component::update_view(&model, &mut widgets, sender);

        ComponentParts { model, widgets }
    }

    fn update(&mut self, state: Self::Input, sender: ComponentSender<Self>, _root: &Self::Root) {
        // only highlight real changes, not the first value or repeats of the
        // same value from unrelated updates
        let changed = match (&self.state, &state) {
            (Some(old), Some(new)) => (old.progress - new.progress).abs() > f64::EPSILON,
            _ => false,
        };
        self.state = state;
        if changed {
            self.activate(&sender);
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
        widgets.root.set_visible(self.state.is_some());
        let Some(state) = &self.state else {
            return;
        };

        let mut bar_classes = ATTENTION.with_classes(&[]);
        if self.active {
            bar_classes.push("active");
        }
        let mut icon_classes = bar_classes.clone();
        icon_classes.push("icon");
        widgets.icon.set_css_classes(&icon_classes);
        widgets.progress_bar.set_css_classes(&bar_classes);

        widgets.icon.set_icon_name(Some(&state.icon));
        widgets.progress_bar.set_fraction(state.progress);
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

/// Builds a progress tile that shows `view` of `state` and stays in sync
/// with every write to it, hidden whenever `view` returns `None`.
///
/// # Example
/// ```ignore
/// let tile = state_progress_tile(&BRIGHTNESS_STATE, brightness_view);
/// ```
pub fn state_progress_tile<S>(
    state: &'static SharedState<S>,
    view: fn(&S) -> Option<ProgressState>,
) -> Controller<ProgressTile>
where
    S: Default + Send + Sync + 'static,
{
    let tile = ProgressTile::builder().launch(None);
    // subscribe before the first read so a write in between isn't missed
    state.subscribe(tile.sender(), view);
    tile.emit(view(&state.read()));
    tile.detach()
}
