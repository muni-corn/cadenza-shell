use gtk4::prelude::*;
use relm4::{Worker, prelude::*};

use crate::{
    icon_names,
    services::mpris::{MprisPlaybackStatus, MprisService, MprisWorkerOutput},
};

#[derive(Debug)]
pub struct MprisTile {
    tile: Controller<Tile>,
    _service: Controller<MprisService>,
}

#[derive(Debug)]
pub enum MprisMsg {
    ServiceUpdate(<MprisService as Worker>::Output),
}

impl SimpleComponent for MprisTile {
    type Init = ();
    type Input = MprisMsg;
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
            .forward(sender.input_sender(), MprisMsg::ServiceUpdate);

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

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            MprisMsg::ServiceUpdate(output) => match output {
                MprisWorkerOutput::StateChanged(state) => {
                    // update visibility
                    self.tile.emit(TileMsg::SetVisible(state.has_player));

                    if state.has_player {
                        // choose icon based on playback status
                        let icon = match state.status {
                            MprisPlaybackStatus::Playing => icon_names::MUSIC_NOTE_1_REGULAR,
                            MprisPlaybackStatus::Paused => icon_names::PAUSE_REGULAR,
                            MprisPlaybackStatus::Stopped => icon_names::STOP_REGULAR,
                        };

                        self.tile.emit(TileMsg::SetIcon(Some(icon.to_string())));

                        // update title (primary text)
                        let title = if state.title.is_empty() {
                            "No title".to_string()
                        } else if state.title.len() > 20 {
                            format!("{}…", &state.title[..17])
                        } else {
                            state.title
                        };
                        self.tile.emit(TileMsg::SetPrimary(Some(title)));

                        // update artist (secondary text)
                        let artist = if state.artist.is_empty() {
                            None
                        } else if state.artist.len() > 15 {
                            Some(format!("{}…", &state.artist[..12]))
                        } else {
                            Some(state.artist)
                        };
                        self.tile.emit(TileMsg::SetSecondary(artist));
                    } else {
                        // no player - hide tile
                        self.tile.emit(TileMsg::SetVisible(false));
                    }
                }
                MprisWorkerOutput::Error(error) => {
                    log::error!("MPRIS error: {}", error);
                }
            },
        }
    }

    fn init_root() -> Self::Root {
        gtk::Box::new(gtk::Orientation::Horizontal, 0)
    }
}
