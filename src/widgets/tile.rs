use gtk4::prelude::*;
use relm4::{Component, SharedState, prelude::*};

use crate::{settings, tiles::Attention};

/// What a tile shows.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TileState {
    pub icon: Option<String>,
    pub primary: Option<String>,
    pub secondary: Option<String>,
    pub attention: Attention,
    pub tooltip: Option<String>,
}

/// A bar button with an icon and up to two labels.
#[derive(Clone, Debug)]
pub struct Tile {
    /// `None` hides the tile.
    state: Option<TileState>,
}

#[derive(Debug)]
pub enum TileOutput {
    Clicked,
}

#[derive(Debug)]
pub struct TileWidgets {
    root: gtk::Button,
    icon: gtk::Image,
    primary_label: gtk::Label,
    secondary_label: gtk::Label,
}

impl SimpleComponent for Tile {
    /// The initial state; `None` starts the tile hidden.
    type Init = Option<TileState>;
    /// Replaces everything the tile shows; `None` hides it.
    type Input = Option<TileState>;
    type Output = TileOutput;
    type Root = gtk::Button;
    type Widgets = TileWidgets;

    fn init(
        state: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = Tile { state };

        // create container
        let container = gtk::Box::new(gtk::Orientation::Horizontal, 8);

        // create widgets
        let icon = gtk::Image::builder()
            .css_classes(["icon"])
            .pixel_size(settings::get_config().tiles.icon_size)
            .width_request(settings::get_config().tiles.icon_size)
            .build();

        let primary_label = gtk::Label::builder()
            .css_classes(["primary"])
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .max_width_chars(64)
            .build();

        let secondary_label = gtk::Label::builder()
            .css_classes(["secondary"])
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .max_width_chars(20)
            .build();

        // add all widgets to container
        container.append(&icon);
        container.append(&primary_label);
        container.append(&secondary_label);

        // add the container to the button
        root.set_child(Some(&container));

        let output = sender.output_sender().clone();
        root.connect_clicked(move |_| output.emit(TileOutput::Clicked));

        let mut widgets = TileWidgets {
            root: root.clone(),
            icon,
            primary_label,
            secondary_label,
        };

        // update all the widgets so they have the correct initial information
        SimpleComponent::update_view(&model, &mut widgets, sender);

        ComponentParts { model, widgets }
    }

    fn update(&mut self, state: Self::Input, _sender: ComponentSender<Self>) {
        self.state = state;
    }

    fn update_view(&self, widgets: &mut Self::Widgets, _sender: ComponentSender<Self>) {
        widgets.root.set_visible(self.state.is_some());
        let Some(state) = &self.state else {
            return;
        };

        // update attention css classes
        widgets
            .icon
            .set_css_classes(&state.attention.with_classes(&["icon"]));
        widgets
            .primary_label
            .set_css_classes(&state.attention.with_classes(&["primary"]));
        widgets
            .secondary_label
            .set_css_classes(&state.attention.with_classes(&["secondary"]));

        widgets.icon.set_visible(state.icon.is_some());
        widgets.icon.set_icon_name(state.icon.as_deref());

        set_optional_label(&widgets.primary_label, state.primary.as_deref());
        set_optional_label(&widgets.secondary_label, state.secondary.as_deref());

        widgets.root.set_tooltip_text(state.tooltip.as_deref());
    }

    fn init_root() -> Self::Root {
        gtk::Button::builder().css_classes(["tile"]).build()
    }
}

/// Shows `text` in `label`, hiding the label when there's none.
fn set_optional_label(label: &gtk::Label, text: Option<&str>) {
    label.set_visible(text.is_some());
    label.set_label(text.unwrap_or_default());
}

/// Builds a tile that shows `view` of `state` and stays in sync with every
/// write to it, hidden whenever `view` returns `None`.
///
/// # Example
/// ```ignore
/// let tile = state_tile(&WEATHER_STATE, weather_view).detach();
/// ```
pub fn state_tile<S>(
    state: &'static SharedState<S>,
    view: fn(&S) -> Option<TileState>,
) -> relm4::component::Connector<Tile>
where
    S: Default + Send + Sync + 'static,
{
    let tile = Tile::builder().launch(None);
    // subscribe before the first read so a write in between isn't missed
    state.subscribe(tile.sender(), view);
    tile.emit(view(&state.read()));
    tile
}
