use gtk4::prelude::*;
use relm4::prelude::*;

use crate::messages::{NetworkType, TileOutput};
use crate::services::network::{DeviceType, NetworkService};
use crate::utils::icons::{NETWORK_WIFI_ICONS, percentage_to_icon_from_list};

#[derive(Debug)]
pub enum NetworkType {
    Ethernet,
    Mobile,
    Vpn,
    Wifi,
    None,
}

#[derive(Debug)]
pub struct NetworkTile {
    connected: bool,
    connection_type: NetworkType,
    signal_strength: Option<f64>,
    ssid: Option<String>,
    service: NetworkService,
}

#[derive(Debug)]
pub enum NetworkMsg {
    Click,
    RightClick,
    ServiceUpdate {
        connected: bool,
        connection_type: NetworkType,
        signal_strength: Option<f64>,
        ssid: Option<String>,
    },
}

#[relm4::component(pub)]
impl SimpleComponent for NetworkTile {
    type Init = ();
    type Input = NetworkMsg;
    type Output = TileOutput;

    view! {
        #[root]
        tile_button = gtk::Button {
            add_css_class: "tile",
            add_css_class: "network",

            connect_clicked[sender] => move |_| {
                sender.input(NetworkMsg::Click);
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
        let service = NetworkService::new();

        let model = NetworkTile {
            connected: false,
            connection_type: NetworkType::None,
            signal_strength: None,
            ssid: None,
            service: service.clone(),
        };

        let widgets = view_output!();

        // Connect to network service updates
        service.connect_connected_notify(glib::clone!(
            #[strong]
            sender,
            move |service| {
                let connection_type = match service.primary_device_type() {
                    DeviceType::Wifi => NetworkType::Wifi,
                    DeviceType::Ethernet => NetworkType::Ethernet,
                    DeviceType::Bluetooth => NetworkType::Mobile,
                    DeviceType::Generic => NetworkType::None,
                    DeviceType::Unknown => NetworkType::None,
                };

                let signal_strength = if service.wifi_enabled() {
                    Some(service.wifi_strength() as f64)
                } else {
                    None
                };

                let ssid = if service.wifi_enabled() && !service.wifi_ssid().is_empty() {
                    Some(service.wifi_ssid())
                } else {
                    None
                };

                sender.input(NetworkMsg::ServiceUpdate {
                    connected: service.connected(),
                    connection_type,
                    signal_strength,
                    ssid,
                });
            }
        ));

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            NetworkMsg::Click => {
                sender
                    .output(TileOutput::Clicked("network".to_string()))
                    .ok();
            }
            NetworkMsg::RightClick => {
                // Could show network menu
            }
            NetworkMsg::ServiceUpdate {
                connected,
                connection_type,
                signal_strength,
                ssid,
            } => {
                self.connected = connected;
                self.connection_type = connection_type;
                self.signal_strength = signal_strength;
                self.ssid = ssid;
            }
        }
    }
}

impl NetworkTile {
    fn get_icon(&self) -> String {
        if !self.connected {
            return "network-offline-symbolic".to_string();
        }

        match self.connection_type {
            NetworkType::Wifi => {
                if let Some(strength) = self.signal_strength {
                    percentage_to_icon_from_list(strength, NETWORK_WIFI_ICONS).to_string()
                } else {
                    "network-wireless-symbolic".to_string()
                }
            }
            NetworkType::Ethernet => "network-wired-symbolic".to_string(),
            NetworkType::Mobile => "network-cellular-symbolic".to_string(),
            NetworkType::Vpn => "network-vpn-symbolic".to_string(),
            NetworkType::None => "network-offline-symbolic".to_string(),
        }
    }

    fn get_text(&self) -> String {
        match &self.connection_type {
            NetworkType::Wifi => {
                if let Some(ssid) = &self.ssid {
                    ssid.clone()
                } else if self.connected {
                    "WiFi".to_string()
                } else {
                    "".to_string()
                }
            }
            NetworkType::Ethernet => {
                if self.connected {
                    "Ethernet".to_string()
                } else {
                    "".to_string()
                }
            }
            NetworkType::Mobile => {
                if self.connected {
                    "Mobile".to_string()
                } else {
                    "".to_string()
                }
            }
            NetworkType::Vpn => {
                if self.connected {
                    "VPN".to_string()
                } else {
                    "".to_string()
                }
            }
            NetworkType::None => "Disconnected".to_string(),
        }
    }
}
