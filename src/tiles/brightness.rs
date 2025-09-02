use gtk4::prelude::*;
use relm4::prelude::*;

use crate::{
    services::brightness::BrightnessService,
    utils::icons::{BRIGHTNESS_ICON_NAMES, percentage_to_icon_from_list},
    widgets::tile::TileOutput,
};

#[derive(Debug)]
pub struct BrightnessTile {
    brightness: f64,
    available: bool,
    _service: BrightnessService,
}

#[derive(Debug)]
pub enum BrightnessMsg {
    Click,
    ServiceUpdate { brightness: f64, available: bool },
}

#[relm4::component(pub)]
impl SimpleComponent for BrightnessTile {
    type Init = ();
    type Input = BrightnessMsg;
    type Output = TileOutput;

    view! {
        #[root]
        tile_button = gtk::Button {
            add_css_class: "tile",
            add_css_class: "brightness",
            #[watch]
            set_visible: model.available,

            connect_clicked[sender] => move |_| {
                sender.input(BrightnessMsg::Click);
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
        let service = BrightnessService::new();

        let model = BrightnessTile {
            brightness: 0.0,
            available: false,
            _service: service.clone(),
        };

        let widgets = view_output!();

        // Connect to brightness service
        service.connect_brightness_notify(glib::clone!(
            #[strong]
            sender,
            move |service| {
                sender.input(BrightnessMsg::ServiceUpdate {
                    brightness: service.brightness(),
                    available: service.available(),
                });
            }
        ));

        // Initial update
        if service.available() {
            sender.input(BrightnessMsg::ServiceUpdate {
                brightness: service.brightness(),
                available: service.available(),
            });
        }

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            BrightnessMsg::Click => {
                sender.output(TileOutput::Clicked).ok();
            }
            BrightnessMsg::ServiceUpdate {
                brightness,
                available,
            } => {
                self.brightness = brightness;
                self.available = available;
            }
        }
    }
}

impl BrightnessTile {
    fn get_icon(&self) -> &str {
        if !self.available {
            "display-brightness-symbolic"
        } else {
            let log_brightness = self.get_logarithmic_brightness();
            percentage_to_icon_from_list(log_brightness, BRIGHTNESS_ICON_NAMES)
        }
    }

    fn get_logarithmic_brightness(&self) -> f64 {
        // use logarithmic scale for perceived brightness
        // convert linear brightness to logarithmic perception
        if self.brightness <= 0.0 {
            0.0
        } else {
            // logarithmic mapping: log10(brightness * 9.0 + 1.0) gives us 0-1 range
            (self.brightness * 9.0 + 1.0).log10()
        }
    }

    fn get_text(&self) -> String {
        if !self.available {
            return "N/A".to_string();
        }

        format!(
            "{}%",
            (self.get_logarithmic_brightness() * 100.0).round() as u32
        )
    }
}
