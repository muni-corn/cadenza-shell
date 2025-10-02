use gtk4::prelude::*;
use relm4::prelude::*;

use crate::{
    icon_names::{MUSIC_NOTE_1_REGULAR, PAUSE_REGULAR},
    services::mpris::{MPRIS_STATE, MprisState},
    widgets::tile::{Tile, TileMsg},
};

#[derive(Debug)]
pub struct MprisTile {
    state: Option<MprisState>,
}

#[derive(Debug)]
pub struct MprisWidgets {
    root: <MprisTile as Component>::Root,
    tile: Controller<Tile>,
}

impl SimpleComponent for MprisTile {
    type Init = ();
    type Input = Option<MprisState>;
    type Output = ();
    type Root = gtk::Box;
    type Widgets = MprisWidgets;

    fn init(
        _init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        MPRIS_STATE.subscribe(sender.input_sender(), |data| data.clone());
        sender.input(MPRIS_STATE.read().clone());

        // initialize the tile component
        let tile = Tile::builder().launch(Default::default()).detach();

        root.append(tile.widget());

        let model = MprisTile { state: None };

        ComponentParts {
            model,
            widgets: MprisWidgets { root, tile },
        }
    }

    fn update(&mut self, state: Self::Input, _sender: ComponentSender<Self>) {
        self.state = state
    }

    fn update_view(&self, widgets: &mut Self::Widgets, _sender: ComponentSender<Self>) {
        match &self.state {
            None => widgets.root.set_visible(false),
            Some(MprisState {
                title,
                artist,
                status,
            }) => {
                let icon = match status {
                    mpris::PlaybackStatus::Playing => MUSIC_NOTE_1_REGULAR,
                    _ => PAUSE_REGULAR,
                }
                .to_string();

                widgets.tile.emit(TileMsg::SetIcon(Some(icon)));
                widgets.tile.emit(TileMsg::SetPrimary(title.clone()));
                widgets.tile.emit(TileMsg::SetSecondary(artist.clone()));
                widgets.root.set_visible(true);
            }
        }
    }

    fn init_root() -> Self::Root {
        gtk::Box::builder().visible(false).build()
    }
}
