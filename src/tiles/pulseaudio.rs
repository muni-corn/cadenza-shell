use gtk4::prelude::*;
use relm4::{WorkerController, prelude::*};

use crate::{
    services::pulseaudio::{
        PulseAudioData, PulseAudioService, PulseAudioServiceEvent, PulseAudioServiceMsg,
    },
    utils::icons::{VOLUME_ICONS, VOLUME_MUTED, VOLUME_ZERO, percentage_to_icon_from_list},
    widgets::progress_tile::{ProgressTile, ProgressTileInit, ProgressTileMsg, ProgressTileOutput},
};

#[derive(Debug)]
pub struct PulseAudioTile {
    volume_data: PulseAudioData,
    worker: WorkerController<PulseAudioService>,
    progress_tile: Controller<ProgressTile>,
}

#[derive(Debug)]
pub enum PulseAudioTileMsg {
    ServiceUpdate(PulseAudioServiceEvent),
    TileClicked,
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
        let worker = PulseAudioService::builder()
            .detach_worker(())
            .forward(sender.input_sender(), PulseAudioTileMsg::ServiceUpdate);

        let volume_data = PulseAudioData::default();

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

        let model = PulseAudioTile {
            volume_data,
            worker,
            progress_tile,
        };

        ComponentParts {
            model,
            widgets: root,
        }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            PulseAudioTileMsg::TileClicked => {
                self.worker.emit(PulseAudioServiceMsg::ToggleMute);
            }
            PulseAudioTileMsg::ServiceUpdate(output) => match output {
                PulseAudioServiceEvent::VolumeChanged(data) => {
                    self.volume_data = data;
                    self.update_tile_data();
                }
                PulseAudioServiceEvent::Error(error) => {
                    log::error!("volume worker error: {}", error);
                }
            },
        }
    }

    fn update_view(&self, root: &mut Self::Widgets, _sender: ComponentSender<Self>) {
        root.set_visible(self.volume_data.default_sink_name.is_some());
    }

    fn init_root() -> Self::Root {
        gtk::Box::builder().visible(false).build()
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

    fn update_tile_data(&mut self) {
        self.progress_tile
            .emit(ProgressTileMsg::SetIcon(Some(self.get_icon().to_string())));

        self.progress_tile
            .emit(ProgressTileMsg::SetProgress(if self.volume_data.muted {
                0.0
            } else {
                (self.volume_data.volume / 100.0).clamp(0.0, 1.0)
            }));
    }
}
