use gtk4::prelude::*;
use relm4::prelude::*;

use crate::messages::TileOutput;

const MPRIS_PLAYING_ICON: &str = "󰐊";
const MPRIS_PAUSED_ICON: &str = "󰏤";
const MPRIS_STOPPED_ICON: &str = "󰓛";

#[derive(Debug, Clone)]
pub enum PlaybackStatus {
    Playing,
    Paused,
    Stopped,
}

impl Default for PlaybackStatus {
    fn default() -> Self {
        Self::Stopped
    }
}

#[derive(Debug)]
pub struct MprisWidget {
    title: String,
    artist: String,
    status: PlaybackStatus,
    has_player: bool,
}

#[derive(Debug)]
pub enum MprisMsg {
    Click,
    UpdateMedia(String, String, PlaybackStatus), // title, artist, status
    PlayerAvailable(bool),
}

#[relm4::component(pub)]
impl SimpleComponent for MprisWidget {
    type Init = ();
    type Input = MprisMsg;
    type Output = TileOutput;

    view! {
        #[root]
        tile_button = gtk::Button {
            add_css_class: "tile",
            add_css_class: "mpris",
            #[watch]
            set_visible: model.has_player,

            connect_clicked[sender] => move |_| {
                sender.input(MprisMsg::Click);
            },

            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 8,
                set_halign: gtk::Align::Center,

                gtk::Label {
                    #[watch]
                    set_label: &model.get_status_icon(),
                    add_css_class: "tile-icon",
                },

                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 2,

                    gtk::Label {
                        #[watch]
                        set_text: &model.get_display_title(),
                        add_css_class: "tile-text",
                        add_css_class: "mpris-title",
                        set_ellipsize: gtk::pango::EllipsizeMode::End,
                        set_max_width_chars: 20,
                    },

                    gtk::Label {
                        #[watch]
                        set_text: &model.artist,
                        add_css_class: "tile-text",
                        add_css_class: "mpris-artist",
                        set_ellipsize: gtk::pango::EllipsizeMode::End,
                        set_max_width_chars: 20,
                        #[watch]
                        set_visible: !model.artist.is_empty(),
                    },
                },
            }
        }
    }

    fn init(
        _init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = MprisWidget {
            title: "No media playing".to_string(),
            artist: String::new(),
            status: PlaybackStatus::default(),
            has_player: false,
        };

        let widgets = view_output!();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            MprisMsg::Click => {
                log::debug!("MPRIS tile clicked");
                let _ = sender.output(TileOutput::Clicked("mpris".to_string()));
            }
            MprisMsg::UpdateMedia(title, artist, status) => {
                self.title = title;
                self.artist = artist;
                self.status = status;
            }
            MprisMsg::PlayerAvailable(available) => {
                self.has_player = available;
                if !available {
                    self.title = "No media playing".to_string();
                    self.artist = String::new();
                    self.status = PlaybackStatus::Stopped;
                }
            }
        }
    }
}

impl MprisWidget {
    fn get_status_icon(&self) -> String {
        match self.status {
            PlaybackStatus::Playing => MPRIS_PLAYING_ICON.to_string(),
            PlaybackStatus::Paused => MPRIS_PAUSED_ICON.to_string(),
            PlaybackStatus::Stopped => MPRIS_STOPPED_ICON.to_string(),
        }
    }

    fn get_display_title(&self) -> String {
        if self.title.is_empty() || self.title == "No media playing" {
            "No media".to_string()
        } else {
            self.title.clone()
        }
    }
}

pub fn create_mpris_widget() -> gtk4::Widget {
    let controller = MprisWidget::builder().launch(()).detach();
    controller.widget().clone().into()
}