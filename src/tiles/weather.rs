use gtk4::prelude::*;
use relm4::prelude::*;

use crate::{
    services::weather::WEATHER_STATE,
    widgets::tile::{Tile, TileMsg},
};

#[derive(Debug)]
pub struct WeatherTile;

#[derive(Debug)]
pub struct WeatherWidgets {
    root: <WeatherTile as Component>::Root,
    tile: Controller<Tile>,
}

impl SimpleComponent for WeatherTile {
    type Init = ();
    type Input = ();
    type Output = ();
    type Root = gtk::Box;
    type Widgets = WeatherWidgets;

    fn init(
        _init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        // subscribe to the global weather state
        WEATHER_STATE.subscribe(sender.input_sender(), |_| ());

        // Initialize the Tile component
        let tile = Tile::builder().launch(Default::default()).detach();

        root.append(tile.widget());

        ComponentParts {
            model: Self,
            widgets: WeatherWidgets { root, tile },
        }
    }

    fn update(&mut self, _msg: Self::Input, _sender: ComponentSender<Self>) {}

    fn update_view(&self, widgets: &mut Self::Widgets, _sender: ComponentSender<Self>) {
        if let Some(data) = WEATHER_STATE.read().clone() {
            // Update the tile with new data
            widgets.tile.emit(TileMsg::SetIcon(Some(data.icon)));
            widgets
                .tile
                .emit(TileMsg::SetPrimary(Some(format!("{}°", data.temperature))));
            widgets
                .tile
                .emit(TileMsg::SetSecondary(Some(data.condition)));
            widgets.root.set_visible(true);
        } else {
            widgets.root.set_visible(false);
        }
    }

    fn init_root() -> Self::Root {
        gtk::Box::builder().visible(false).build()
    }
}
