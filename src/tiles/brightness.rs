use gtk4::prelude::*;
use relm4::{Worker, prelude::*};

use crate::{
    services::brightness::{BrightnessEvent, BrightnessService},
    utils::icons::{BRIGHTNESS_ICON_NAMES, percentage_to_icon_from_list},
    widgets::tile::{Tile, TileInit, TileMsg},
};

pub struct BrightnessTile {
    brightness_percentage: Option<f64>,
    _service: Controller<BrightnessService>,
    tile: Controller<Tile>,
}

#[derive(Debug)]
pub enum BrightnessMsg {
    ServiceUpdate(<BrightnessService as Worker>::Output),
}

impl SimpleComponent for BrightnessTile {
    type Init = ();
    type Input = BrightnessMsg;
    type Output = ();
    type Root = gtk::Box;
    type Widgets = ();

    fn init(
        _init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let _service = BrightnessService::builder()
            .launch(())
            .forward(sender.input_sender(), |state| {
                BrightnessMsg::ServiceUpdate(state)
            });

        // initialize the tile component
        let tile = Tile::builder()
            .launch(TileInit {
                name: "brightness".to_string(),
                visible: false,
                ..Default::default()
            })
            .detach();

        root.append(tile.widget());

        let model = BrightnessTile {
            brightness_percentage: None,
            _service,
            tile,
        };

        ComponentParts { model, widgets: () }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            BrightnessMsg::ServiceUpdate(state) => match state {
                BrightnessEvent::Percentage(p) => {
                    self.brightness_percentage = Some(p);

                    // update the tile with new data
                    self.tile.emit(TileMsg::UpdateData {
                        icon: Some(self.get_icon().to_string()),
                        primary: Some(self.get_text()),
                        secondary: None,
                    });

                    // update visibility
                    self.tile.emit(TileMsg::SetVisible(true));
                }
                BrightnessEvent::Unavailable => {
                    self.brightness_percentage = None;

                    // hide the tile when brightness is unavailable
                    self.tile.emit(TileMsg::SetVisible(false));
                }
            },
        }
    }

    fn init_root() -> Self::Root {
        gtk::Box::new(gtk::Orientation::Horizontal, 0)
    }
}

impl BrightnessTile {
    /// Gets the icon for this tile, using a logarithmic curve for brightness.
    fn get_icon(&self) -> &str {
        self.brightness_percentage
            .as_ref()
            .map(|p| {
                let log_brightness = to_logarithmic(*p);
                percentage_to_icon_from_list(log_brightness, BRIGHTNESS_ICON_NAMES)
            })
            .unwrap_or_default()
    }

    /// Gets the percentage text for this tile, using a logarithmic curve for
    /// brightness.
    fn get_text(&self) -> String {
        self.brightness_percentage
            .as_ref()
            .map(|p| format!("{}%", (to_logarithmic(*p) * 100.0).round() as u32))
            .unwrap_or_default()
    }
}

fn to_logarithmic(brightness: f64) -> f64 {
    // use logarithmic scale for perceived brightness
    // convert linear brightness to logarithmic perception
    if brightness <= 0.0 {
        0.0
    } else {
        // logarithmic mapping: log10(brightness * 9.0 + 1.0) gives us 0-1 range
        (brightness * 9.0 + 1.0).log10()
    }
}
