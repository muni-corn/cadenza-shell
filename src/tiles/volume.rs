use gtk4::prelude::*;
use relm4::prelude::*;

use crate::{services::audio::AudioService, widgets::tile::TileOutput};

#[derive(Debug)]
pub struct VolumeTile {
    volume: f64,
    muted: bool,
    available: bool,
    service: AudioService,
}

#[derive(Debug)]
pub enum VolumeMsg {
    Click,
    RightClick,
    Scroll(f64), // delta for volume adjustment
    ServiceUpdate {
        volume: f64,
        muted: bool,
        available: bool,
    },
}

#[relm4::component(pub)]
impl SimpleComponent for VolumeTile {
    type Init = ();
    type Input = VolumeMsg;
    type Output = TileOutput;

    view! {
        #[root]
        tile_button = gtk::Button {
            add_css_class: "tile",
            add_css_class: "volume",
            #[watch]
            set_visible: model.available,

            connect_clicked[sender] => move |_| {
                sender.input(VolumeMsg::Click);
            },

            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 8,
                set_halign: gtk::Align::Center,

                gtk::Image {
                    #[watch]
                    set_icon_name: Some(&model.get_icon()),
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
        let service = AudioService::new();

        let model = VolumeTile {
            volume: 0.0,
            muted: false,
            available: false,
            service: service.clone(),
        };

        let widgets = view_output!();

        // Connect to audio service updates
        service.connect_volume_notify(glib::clone!(
            #[strong]
            sender,
            move |service| {
                sender.input(VolumeMsg::ServiceUpdate {
                    volume: service.volume(),
                    muted: service.muted(),
                    available: service.available(),
                });
            }
        ));

        service.connect_muted_notify(glib::clone!(
            #[strong]
            sender,
            move |service| {
                sender.input(VolumeMsg::ServiceUpdate {
                    volume: service.volume(),
                    muted: service.muted(),
                    available: service.available(),
                });
            }
        ));

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            VolumeMsg::Click => {
                // Toggle mute
                self.service.set_muted(!self.muted);
                sender.output(TileOutput::Clicked).ok();
            }
            VolumeMsg::RightClick => {
                // Could show volume mixer
            }
            VolumeMsg::Scroll(delta) => {
                let new_volume = (self.volume + delta * 0.05).clamp(0.0, 1.0);
                self.service.set_volume(new_volume);
            }
            VolumeMsg::ServiceUpdate {
                volume,
                muted,
                available,
            } => {
                self.volume = volume;
                self.muted = muted;
                self.available = available;
            }
        }
    }
}

impl VolumeTile {
    fn get_icon(&self) -> String {
        if !self.available {
            return "audio-volume-muted-symbolic".to_string();
        }

        if self.muted || self.volume == 0.0 {
            "audio-volume-muted-symbolic".to_string()
        } else if self.volume > 0.8 {
            "audio-volume-high-symbolic".to_string()
        } else if self.volume > 0.3 {
            "audio-volume-medium-symbolic".to_string()
        } else {
            "audio-volume-low-symbolic".to_string()
        }
    }

    fn get_text(&self) -> String {
        if !self.available {
            return "N/A".to_string();
        }

        if self.muted {
            "Muted".to_string()
        } else {
            format!("{}%", (self.volume * 100.0) as u32)
        }
    }
}
