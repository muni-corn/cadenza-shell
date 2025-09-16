use gtk4::prelude::*;
use relm4::{WorkerController, prelude::*};

use crate::{
    services::pulseaudio::{
        PulseAudioData, PulseAudioService, PulseAudioServiceEvent, PulseAudioServiceMsg,
    },
    utils::icons::{VOLUME_ICONS, VOLUME_MUTED, VOLUME_ZERO, percentage_to_icon_from_list},
    widgets::tile::{Attention, Tile, TileInit, TileMsg, TileOutput},
};

#[derive(Debug)]
pub struct PulseAudioTile {
    volume_data: PulseAudioData,
    worker: WorkerController<PulseAudioService>,
    tile: Controller<Tile>,
}

#[derive(Debug)]
pub enum PulseAudioTileMsg {
    ServiceUpdate(PulseAudioServiceEvent),
    TileClicked,
}

impl SimpleComponent for PulseAudioTile {
    type Init = ();
    type Input = PulseAudioTileMsg;
    type Output = TileOutput;
    type Root = gtk::Box;
    type Widgets = ();

    fn init(
        _init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let worker = PulseAudioService::builder()
            .detach_worker(())
            .forward(sender.input_sender(), PulseAudioTileMsg::ServiceUpdate);

        let volume_data = PulseAudioData::default();

        let tile = Tile::builder()
            .launch(TileInit {
                name: "pulseaudio".to_string(),
                icon_name: None,
                primary: None,
                secondary: None,
                visible: volume_data.default_sink_name.is_some(),
                attention: Attention::Normal,
                extra_classes: vec!["volume".to_string()],
            })
            .forward(sender.input_sender(), |output| match output {
                TileOutput::Clicked => PulseAudioTileMsg::TileClicked,
                _ => PulseAudioTileMsg::TileClicked,
            });
        root.append(tile.widget());

        let model = PulseAudioTile {
            volume_data,
            worker,
            tile,
        };

        ComponentParts { model, widgets: () }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            PulseAudioTileMsg::TileClicked => {
                self.worker
                    .sender()
                    .send(PulseAudioServiceMsg::ToggleMute)
                    .ok();
                sender.output(TileOutput::Clicked).ok();
            }
            PulseAudioTileMsg::ServiceUpdate(output) => match output {
                PulseAudioServiceEvent::VolumeChanged(data) => {
                    self.volume_data = data;
                    self.update_tile_display();
                }
                PulseAudioServiceEvent::Error(error) => {
                    log::error!("volume worker error: {}", error);
                }
            },
        }
    }

    fn init_root() -> Self::Root {
        gtk::Box::builder().build()
    }
}

impl PulseAudioTile {
    fn get_icon(&self) -> &str {
        if self.volume_data.default_sink_name.is_none() || self.volume_data.muted {
            VOLUME_MUTED
        } else if self.volume_data.volume == 0.0 {
            VOLUME_ZERO
        } else {
            percentage_to_icon_from_list(self.volume_data.volume / 100.0, VOLUME_ICONS)
        }
    }

    fn get_text(&self) -> String {
        if self.volume_data.default_sink_name.is_none() {
            return "N/A".to_string();
        }

        if self.volume_data.muted {
            "Muted".to_string()
        } else {
            format!("{}%", self.volume_data.volume as u32)
        }
    }

    fn update_tile_display(&mut self) {
        self.tile
            .sender()
            .send(TileMsg::UpdateData {
                icon: Some(self.get_icon().to_string()),
                primary: Some(self.get_text()),
                secondary: None,
            })
            .ok();

        self.tile
            .sender()
            .send(TileMsg::SetVisible(
                self.volume_data.default_sink_name.is_some(),
            ))
            .ok();
    }
}
