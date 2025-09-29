use gtk4::prelude::*;
use relm4::prelude::*;

use crate::{
    services::pulseaudio::{PulseAudioData, VOLUME_STATE},
    utils::icons::{VOLUME_ICONS, VOLUME_MUTED, VOLUME_ZERO, percentage_to_icon_from_list},
    widgets::progress_tile::{ProgressTile, ProgressTileInit, ProgressTileMsg, ProgressTileOutput},
};

#[derive(Debug)]
pub struct PulseAudioTile {
    progress_tile: Controller<ProgressTile>,
}

#[derive(Debug)]
pub enum PulseAudioTileMsg {
    TileClicked,
    Update,
}

impl SimpleComponent for PulseAudioTile {
    type Init = ();
    type Input = PulseAudioTileMsg;
    type Output = ();
    type Root = gtk::Box;
    type Widgets = Self::Root;

    fn init(
        _init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        VOLUME_STATE.subscribe(sender.input_sender(), |_| PulseAudioTileMsg::Update);

        let progress_tile = ProgressTile::builder()
            .launch(ProgressTileInit {
                icon_name: None,
                progress: 0.0,
                attention: super::Attention::Dim,
                ..Default::default()
            })
            .forward(sender.input_sender(), |output| match output {
                ProgressTileOutput::Clicked => PulseAudioTileMsg::TileClicked,
            });

        root.append(progress_tile.widget());

        let model = PulseAudioTile { progress_tile };

        // initialize this tile
        sender.input(PulseAudioTileMsg::Update);

        ComponentParts {
            model,
            widgets: root,
        }
    }

    fn update(&mut self, _msg: Self::Input, _sender: ComponentSender<Self>) {}

    fn update_view(&self, root: &mut Self::Widgets, _sender: ComponentSender<Self>) {
        let volume_data = VOLUME_STATE.read().clone();
        self.update_tile_data(&volume_data);
        root.set_visible(VOLUME_STATE.read().default_sink_name.is_some());
    }

    fn init_root() -> Self::Root {
        gtk::Box::builder().visible(false).build()
    }
}

fn get_icon(volume_data: &PulseAudioData) -> &str {
    if volume_data.default_sink_name.is_none() || volume_data.muted {
        VOLUME_MUTED
    } else if volume_data.volume == 0.0 {
        VOLUME_ZERO
    } else {
        percentage_to_icon_from_list(volume_data.volume / 100.0, VOLUME_ICONS)
    }
}

impl PulseAudioTile {
    fn update_tile_data(&self, volume_data: &PulseAudioData) {
        self.progress_tile.emit(ProgressTileMsg::SetIcon(Some(
            get_icon(volume_data).to_string(),
        )));

        self.progress_tile
            .emit(ProgressTileMsg::SetProgress(if volume_data.muted {
                0.0
            } else {
                (volume_data.volume / 100.0).clamp(0.0, 1.0)
            }));
    }
}
