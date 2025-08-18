use gtk4::prelude::*;
use relm4::prelude::*;

use crate::messages::TileOutput;

#[derive(Debug)]
pub struct BluetoothTile {
    enabled: bool,
    connected_devices: u32,
    available: bool,
}

#[derive(Debug)]
pub enum BluetoothMsg {
    Click,
    Toggle,
    ServiceUpdate {
        enabled: bool,
        connected_devices: u32,
        available: bool,
    },
}

#[relm4::component(pub)]
impl SimpleComponent for BluetoothTile {
    type Init = ();
    type Input = BluetoothMsg;
    type Output = TileOutput;

    view! {
        #[root]
        tile_button = gtk::Button {
            add_css_class: "tile",
            add_css_class: "bluetooth",
            #[watch]
            set_visible: model.available,

            connect_clicked[sender] => move |_| {
                sender.input(BluetoothMsg::Click);
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
        let model = BluetoothTile {
            enabled: false,
            connected_devices: 0,
            available: true, // TODO: detect bluetooth availability
        };

        let widgets = view_output!();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            BluetoothMsg::Click => {
                sender
                    .output(TileOutput::Clicked("bluetooth".to_string()))
                    .ok();
            }
            BluetoothMsg::Toggle => {
                self.enabled = !self.enabled;
            }
            BluetoothMsg::ServiceUpdate {
                enabled,
                connected_devices,
                available,
            } => {
                self.enabled = enabled;
                self.connected_devices = connected_devices;
                self.available = available;
            }
        }
    }
}

impl BluetoothTile {
    fn get_icon(&self) -> String {
        if !self.available {
            return "bluetooth-disabled-symbolic".to_string();
        }

        if self.enabled {
            if self.connected_devices > 0 {
                "bluetooth-active-symbolic".to_string()
            } else {
                "bluetooth-symbolic".to_string()
            }
        } else {
            "bluetooth-disabled-symbolic".to_string()
        }
    }

    fn get_text(&self) -> String {
        if !self.available {
            return "N/A".to_string();
        }

        if self.enabled && self.connected_devices > 0 {
            self.connected_devices.to_string()
        } else {
            "".to_string()
        }
        .to_string()
    }
}
