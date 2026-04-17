use gtk4::prelude::*;
use relm4::prelude::*;

use crate::{
    brightness::BRIGHTNESS_STATE,
    utils::icons::{BRIGHTNESS_ICON_NAMES, percentage_to_icon_from_list},
    widgets::progress_tile::{ProgressTile, ProgressTileInit, ProgressTileMsg},
};

#[derive(Debug)]
pub struct BrightnessTile {
    progress_tile: Controller<ProgressTile>,
}

impl SimpleComponent for BrightnessTile {
    type Init = ();
    type Input = ();
    type Output = ();
    type Root = gtk::Box;
    type Widgets = Self::Root;

    fn init(
        _init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        BRIGHTNESS_STATE.subscribe(sender.input_sender(), |_| ());

        // initialize the progress tile component
        let progress_tile = ProgressTile::builder()
            .launch(ProgressTileInit {
                attention: super::Attention::Dim,
                ..Default::default()
            })
            .detach();

        root.append(progress_tile.widget());

        let model = BrightnessTile { progress_tile };

        // inits the tile in case it missed the initialization from the
        // BrightnessService
        sender.input(());

        ComponentParts {
            model,
            widgets: root,
        }
    }

    fn update(&mut self, _msg: Self::Input, _sender: ComponentSender<Self>) {}

    fn update_view(&self, root: &mut Self::Widgets, _sender: ComponentSender<Self>) {
        if let Some(p) = *BRIGHTNESS_STATE.read() {
            // update the progress tile with new data
            self.progress_tile
                .emit(ProgressTileMsg::SetIcon(Some(self.get_icon().to_string())));
            self.progress_tile.emit(ProgressTileMsg::SetProgress(p));

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
        BRIGHTNESS_STATE
            .read()
            .as_ref()
            .map(|p| percentage_to_icon_from_list(*p, BRIGHTNESS_ICON_NAMES))
            .unwrap_or_default()
    }
}
