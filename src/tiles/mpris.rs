use gtk4::prelude::*;
use mpris::PlaybackStatus;
use relm4::prelude::*;

use crate::{
    icon_names,
    services::mpris::{MprisService, MprisState},
    widgets::tile::{Tile, TileInit, TileMsg},
};

#[derive(Debug)]
pub struct MprisTile {
    tile: Controller<Tile>,
    _service: Controller<MprisService>,
}

impl SimpleComponent for MprisTile {
    type Init = ();
    type Input = MprisState;
    type Output = ();
    type Root = gtk::Box;
    type Widgets = ();

    fn init(
        _init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let _service = MprisService::builder()
            .launch(())
            .forward(sender.input_sender(), |s| s);

        // initialize the tile component
        let tile = Tile::builder()
            .launch(TileInit {
                icon_name: None,
                primary: None,
                secondary: None,
                visible: false, // initially hidden until we have media
                ..Default::default()
            })
            .detach();

        root.append(tile.widget());

        let model = MprisTile { tile, _service };

        ComponentParts { model, widgets: () }
    }

    fn update(&mut self, state: Self::Input, _sender: ComponentSender<Self>) {
        // update visibility
        self.tile.emit(TileMsg::SetVisible(state.has_player));

        if state.has_player {
            // choose icon based on playback status
            let icon = match state.status {
                PlaybackStatus::Playing => icon_names::MUSIC_NOTE_1_REGULAR,
                PlaybackStatus::Paused => icon_names::PAUSE_REGULAR,
                PlaybackStatus::Stopped => icon_names::STOP_REGULAR,
            };

            self.tile.emit(TileMsg::SetIcon(Some(icon.to_string())));
            self.tile.emit(TileMsg::SetPrimary(state.title));
            self.tile.emit(TileMsg::SetSecondary(state.artist));
        } else {
            // no player - hide tile
            self.tile.emit(TileMsg::SetVisible(false));
        }
    }

    fn init_root() -> Self::Root {
        gtk::Box::new(gtk::Orientation::Horizontal, 0)
    }
}
