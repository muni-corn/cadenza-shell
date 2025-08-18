use gtk4::prelude::*;
use relm4::prelude::*;

use crate::messages::TileOutput;
use crate::services::brightness::BrightnessService;

#[derive(Debug)]
pub struct BrightnessTile {
    level: f64,
    available: bool,
    service: BrightnessService,
}

#[derive(Debug)]
pub enum BrightnessMsg {
    Click,
    Scroll(f64), // delta
    ServiceUpdate { level: f64, available: bool },
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
        let service = BrightnessService::new();

        let model = BrightnessTile {
            level: 0.0,
            available: false,
            service: service.clone(),
        };

        let widgets = view_output!();

        // Connect to brightness service
        service.connect_level_notify(glib::clone!(
            #[weak]
            sender,
            move |service| {
                sender.input(BrightnessMsg::ServiceUpdate {
                    level: service.level(),
                    available: service.available(),
                });
            }
        ));

        // Initial update
        if service.available() {
            sender.input(BrightnessMsg::ServiceUpdate {
                level: service.level(),
                available: service.available(),
            });
        }

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            BrightnessMsg::Click => {
                sender
                    .output(TileOutput::Clicked("brightness".to_string()))
                    .ok();
            }
            BrightnessMsg::Scroll(delta) => {
                let new_level = (self.level + delta * 0.05).clamp(0.0, 1.0);
                self.service.set_level(new_level);
            }
            BrightnessMsg::ServiceUpdate { level, available } => {
                self.level = level;
                self.available = available;
            }
        }
    }
}

impl BrightnessTile {
    fn get_icon(&self) -> String {
        if !self.available {
            return "display-brightness-symbolic".to_string();
        }

        if self.level > 0.8 {
            "display-brightness-high-symbolic".to_string()
        } else if self.level > 0.4 {
            "display-brightness-medium-symbolic".to_string()
        } else {
            "display-brightness-low-symbolic".to_string()
        }
    }

    fn get_text(&self) -> String {
        if !self.available {
            return "N/A".to_string();
        }

        format!("{}%", (self.level * 100.0) as u32)
    }
}
