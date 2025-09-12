use gtk4::prelude::*;
use relm4::{WorkerController, prelude::*};

use crate::{
    services::pulseaudio::{
        PulseAudioData, PulseAudioService, PulseAudioServiceEvent, PulseAudioServiceMsg,
    },
    utils::icons::{MUTE_ICON, VOLUME_ICONS, percentage_to_icon_from_list},
    widgets::tile::TileOutput,
};

#[derive(Debug)]
pub struct PulseAudioTile {
    volume_data: PulseAudioData,
    worker: WorkerController<PulseAudioService>,
}

#[derive(Debug)]
pub enum PulseAudioTileMsg {
    Click,
    ServiceUpdate(PulseAudioServiceEvent),
}

#[relm4::component(pub)]
impl SimpleComponent for PulseAudioTile {
    type Init = ();
    type Input = PulseAudioTileMsg;
    type Output = TileOutput;

    view! {
        #[root]
        tile_button = gtk::Button {
            add_css_class: "tile",
            add_css_class: "volume",
            #[watch]
            set_visible: model.volume_data.default_sink_name.is_some(),

            connect_clicked[sender] => move |_| {
                sender.input(PulseAudioTileMsg::Click);
            },

            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 8,
                set_halign: gtk::Align::Center,

                gtk::Image {
                    #[watch]
                    set_icon_name: Some(model.get_icon()),
                    add_css_class: "tile-icon",
                },

                gtk::Label {
                    #[watch]
                    set_text: &model.get_text(),
                    add_css_class: "tile-text",
                },
            }
        }
    }

    fn init(
        _init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let worker = PulseAudioService::builder()
            .detach_worker(())
            .forward(sender.input_sender(), PulseAudioTileMsg::ServiceUpdate);

        let model = PulseAudioTile {
            volume_data: PulseAudioData::default(),
            worker,
        };

        let widgets = view_output!();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            PulseAudioTileMsg::Click => {
                // toggle mute
                self.worker
                    .sender()
                    .send(PulseAudioServiceMsg::ToggleMute)
                    .ok();
                sender.output(TileOutput::Clicked).ok();
            }
            PulseAudioTileMsg::ServiceUpdate(output) => match output {
                PulseAudioServiceEvent::VolumeChanged(data) => {
                    self.volume_data = data;
                }
                PulseAudioServiceEvent::Error(error) => {
                    log::error!("volume worker error: {}", error);
                }
            },
        }
    }
}

impl PulseAudioTile {
    fn get_icon(&self) -> &str {
        if self.volume_data.default_sink_name.is_none() || self.volume_data.muted {
            MUTE_ICON
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
}
