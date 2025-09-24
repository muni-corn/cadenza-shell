use gtk4::prelude::*;
use relm4::{Worker, prelude::*};

use crate::{
    services::brightness::{BrightnessEvent, BrightnessService},
    utils::icons::{BRIGHTNESS_ICON_NAMES, percentage_to_icon_from_list},
    widgets::progress_tile::{ProgressTile, ProgressTileInit, ProgressTileMsg},
};

pub struct BrightnessTile {
    brightness_percentage: Option<f64>,
    _service: Controller<BrightnessService>,
    progress_tile: Controller<ProgressTile>,
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
    type Widgets = Self::Root;

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

        // initialize the progress tile component
        let progress_tile = ProgressTile::builder()
            .launch(ProgressTileInit {
                attention: super::Attention::Dim,
                ..Default::default()
            })
            .detach();

        root.append(progress_tile.widget());

        let model = BrightnessTile {
            brightness_percentage: None,
            _service,
            progress_tile,
        };

        ComponentParts {
            model,
            widgets: root,
        }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            BrightnessMsg::ServiceUpdate(state) => match state {
                BrightnessEvent::Percentage(p) => {
                    self.brightness_percentage = Some(p);
                }
                BrightnessEvent::Unavailable => {
                    self.brightness_percentage = None;
                }
            },
        }
    }

    fn update_view(&self, root: &mut Self::Widgets, _sender: ComponentSender<Self>) {
        if let Some(p) = self.brightness_percentage {
            // update the progress tile with new data
            self.progress_tile
                .emit(ProgressTileMsg::SetIcon(Some(self.get_icon().to_string())));
            self.progress_tile
                .emit(ProgressTileMsg::SetProgress(to_logarithmic(p)));

            root.set_visible(true);
        } else {
            // hide the tile when brightness is unavailable
            root.set_visible(false);
        }
    }

    fn init_root() -> Self::Root {
        gtk::Box::builder().visible(false).build()
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
