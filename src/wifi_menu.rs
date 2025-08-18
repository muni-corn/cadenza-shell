use gtk4::prelude::*;
use relm4::factory::FactoryVecDeque;
use relm4::prelude::*;

// WiFi menu component that displays available networks and allows connections
#[derive(Debug, Clone)]
pub struct AccessPoint {
    pub ssid: String,
    pub strength: u32, // 0-100
    pub requires_password: bool,
    pub frequency: u32, // in MHz
    pub is_active: bool,
}

#[derive(Debug, Clone)]
pub struct WifiState {
    pub enabled: bool,
    pub connected_ssid: Option<String>,
    pub connectivity: WifiConnectivity,
    pub state: WifiNetworkState,
    pub access_points: Vec<AccessPoint>,
    pub scanning: bool,
}

#[derive(Debug, Clone)]
pub enum WifiConnectivity {
    Full,
    Limited,
    None,
    Portal,
    Unknown,
}

#[derive(Debug, Clone)]
pub enum WifiNetworkState {
    Asleep,
    ConnectedGlobal,
    ConnectedLocal,
    ConnectedSite,
    Connecting,
    Disconnected,
    Disconnecting,
    Unknown,
}

impl Default for WifiState {
    fn default() -> Self {
        Self {
            enabled: false,
            connected_ssid: None,
            connectivity: WifiConnectivity::Unknown,
            state: WifiNetworkState::Unknown,
            access_points: Vec::new(),
            scanning: false,
        }
    }
}

#[derive(Debug)]
struct WiFiMenu {
    wifi_state: WifiState,
    show_password_dialog: Option<String>, // SSID requiring password
    access_points: FactoryVecDeque<AccessPointWidget>,
}

#[derive(Debug)]
pub enum WiFiMenuMsg {
    ToggleWifi(bool),
    ScanNetworks,
    ConnectToNetwork(String),   // SSID
    ShowPasswordDialog(String), // SSID
    HidePasswordDialog,
    ConnectWithPassword(String, String), // SSID, Password
    UpdateState(WifiState),
}

#[derive(Debug)]
pub enum WiFiMenuOutput {
    // Currently no outputs needed
}

impl WiFiMenu {
    fn connectivity_text(&self) -> &'static str {
        match self.wifi_state.connectivity {
            WifiConnectivity::Full => "Full connectivity",
            WifiConnectivity::Limited => "Limited connectivity",
            WifiConnectivity::None => "No connectivity",
            WifiConnectivity::Portal => "Sign-in needed",
            WifiConnectivity::Unknown => "Connectivity unknown",
        }
    }

    fn state_text(&self) -> &'static str {
        match self.wifi_state.state {
            WifiNetworkState::Asleep => "Sleeping",
            WifiNetworkState::ConnectedGlobal => "Global access",
            WifiNetworkState::ConnectedLocal => "Local access only",
            WifiNetworkState::ConnectedSite => "Site access only",
            WifiNetworkState::Connecting => "Connecting",
            WifiNetworkState::Disconnected => "Disconnected",
            WifiNetworkState::Disconnecting => "Disconnecting",
            WifiNetworkState::Unknown => "State unknown",
        }
    }

    fn get_network_icon(&self) -> String {
        use crate::utils::icons::NETWORK_WIFI_ICONS;

        if !self.wifi_state.enabled {
            return "󰤮".to_string(); // disabled icon
        }

        if self.wifi_state.connected_ssid.is_some() {
            // Find the connected access point and show strength-based icon
            if let Some(ap) = self.wifi_state.access_points.iter().find(|ap| ap.is_active) {
                let icons = NETWORK_WIFI_ICONS;
                let index =
                    ((ap.strength as f64 / 100.0) * (icons.len() - 1) as f64).round() as usize;
                return icons.get(index).unwrap_or(&"󰤟").to_string();
            }
        }

        "󰤯".to_string() // disconnected icon
    }
}

#[relm4::component]
impl SimpleComponent for WiFiMenu {
    type Init = WifiState;
    type Input = WiFiMenuMsg;
    type Output = WiFiMenuOutput;

    view! {
        #[root]
        main_box = gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 16,
            set_vexpand: true,

            // Header with WiFi toggle
            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 20,
                set_hexpand: true,
                add_css_class: "content-title",

                gtk::Label {
                    #[watch]
                    set_label: &model.get_network_icon(),
                },

                gtk::Label {
                    set_label: "WiFi",
                    set_halign: gtk::Align::Start,
                    set_hexpand: true,
                },

                gtk::Switch {
                    #[watch]
                    set_active: model.wifi_state.enabled,
                    set_halign: gtk::Align::End,
                    set_valign: gtk::Align::End,

                    connect_state_set[sender] => move |_, state| {
                        sender.input(WiFiMenuMsg::ToggleWifi(state));
                        glib::Propagation::Proceed
                    },
                },
            },

            // Scrollable content
            gtk::ScrolledWindow {
                set_vscrollbar_policy: gtk::PolicyType::Automatic,
                set_hscrollbar_policy: gtk::PolicyType::Never,
                set_vexpand: true,

                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 16,

                    // Status information
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 4,

                        gtk::Label {
                            set_halign: gtk::Align::Start,
                            #[watch]
                            set_visible: model.wifi_state.connected_ssid.is_some(),
                            #[watch]
                            set_label: &model.wifi_state.connected_ssid
                                .as_ref()
                                .map(|ssid| format!("Connected to {}", ssid))
                                .unwrap_or_default(),
                        },

                        gtk::Label {
                            set_halign: gtk::Align::Start,
                            #[watch]
                            set_label: model.connectivity_text(),
                        },

                        gtk::Label {
                            set_halign: gtk::Align::Start,
                            #[watch]
                            set_label: model.state_text(),
                        },
                    },

                    if model.show_password_dialog.is_some() {
                        #[name = "password_dialog"]
                        gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            set_spacing: 8,

                            gtk::Label {
                                #[watch]
                                set_label: &format!("Enter password for {}",
                                    model.show_password_dialog.as_ref().unwrap()),
                            },

                            #[name = "password_entry"]
                            gtk::Entry {
                                set_visibility: false,
                                set_placeholder_text: Some("Password"),
                            },

                            gtk::Box {
                                set_spacing: 8,

                                gtk::Button {
                                    set_label: "Cancel",
                                    connect_clicked[sender] => move |_| {
                                        sender.input(WiFiMenuMsg::HidePasswordDialog);
                                    },
                                },

                                #[name = "connect_button"]
                                gtk::Button {
                                    set_label: "Connect",
                                },
                            },
                        }
                    } else {
                        gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            set_spacing: 8,
                            set_vexpand: true,

                            // Available networks header
                            gtk::Box {
                                set_hexpand: true,

                                gtk::Label {
                                    set_label: "Available networks",
                                    add_css_class: "bold",
                                    set_halign: gtk::Align::Start,
                                    set_hexpand: true,
                                },

                                gtk::Button {
                                    #[watch]
                                    set_sensitive: !model.wifi_state.scanning,
                                    set_halign: gtk::Align::End,
                                    connect_clicked[sender] => move |_| {
                                        sender.input(WiFiMenuMsg::ScanNetworks);
                                    },

                                    if model.wifi_state.scanning {
                                        gtk::Spinner {
                                            set_spinning: true,
                                        }
                                    } else {
                                        gtk::Image {
                                            set_icon_name: Some("view-refresh"),
                                        }
                                    },
                                },
                            },

                            // Access points list
                            #[local_ref]
                            access_points_box -> gtk::Box {
                                set_orientation: gtk::Orientation::Vertical,
                                set_spacing: 4,
                            },
                        }
                    },
                },
            },
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let access_points = FactoryVecDeque::builder()
            .launch(gtk::Box::default())
            .forward(sender.input_sender(), |output| match output {
                AccessPointOutput::Connect(ssid) => WiFiMenuMsg::ConnectToNetwork(ssid),
                AccessPointOutput::RequestPassword(ssid) => WiFiMenuMsg::ShowPasswordDialog(ssid),
            });

        let model = WiFiMenu {
            wifi_state: init,
            show_password_dialog: None,
            access_points,
        };

        let access_points_box = model.access_points.widget();
        let widgets = view_output!();

        // Setup connect button click handler
        widgets.connect_button.connect_clicked({
            let sender = sender.clone();
            let password_entry = widgets.password_entry.clone();
            move |_| {
                let password = password_entry.text().to_string();
                // Get SSID from the dialog state (simplified for now)
                sender.input(WiFiMenuMsg::ConnectWithPassword(
                    "current_ssid".to_string(),
                    password,
                ));
            }
        });

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            WiFiMenuMsg::UpdateState(state) => {
                self.wifi_state = state;

                // Update access points factory
                let mut guard = self.access_points.guard();
                guard.clear();
                for ap in &self.wifi_state.access_points {
                    guard.push_back(ap.clone());
                }
            }
            WiFiMenuMsg::ToggleWifi(enabled) => {
                // TODO: Implement actual WiFi toggle
                log::info!("toggle wifi: {}", enabled);
            }
            WiFiMenuMsg::ScanNetworks => {
                // TODO: Implement network scan
                log::info!("scanning for networks");
            }
            WiFiMenuMsg::ConnectToNetwork(ssid) => {
                // TODO: Implement connection logic
                log::info!("connecting to network: {}", ssid);
            }
            WiFiMenuMsg::ShowPasswordDialog(ssid) => {
                self.show_password_dialog = Some(ssid);
            }
            WiFiMenuMsg::HidePasswordDialog => {
                self.show_password_dialog = None;
            }
            WiFiMenuMsg::ConnectWithPassword(ssid, _password) => {
                // TODO: Implement password connection
                log::info!("connecting to {} with password", ssid);
                self.show_password_dialog = None;
            }
        }
    }
}

// Factory for individual access point items
#[derive(Debug)]
struct AccessPointWidget {
    access_point: AccessPoint,
}

#[derive(Debug)]
pub enum AccessPointMsg {
    Connect,
}

#[derive(Debug)]
pub enum AccessPointOutput {
    Connect(String),         // SSID
    RequestPassword(String), // SSID
}

#[relm4::factory]
impl FactoryComponent for AccessPointWidget {
    type Init = AccessPoint;
    type Input = AccessPointMsg;
    type Output = AccessPointOutput;
    type CommandOutput = ();
    type ParentWidget = gtk::Box;

    view! {
        gtk::Button {
            connect_clicked[sender] => move |_| {
                sender.input(AccessPointMsg::Connect);
            },

            gtk::Box {
                set_spacing: 8,
                set_halign: gtk::Align::Start,
                set_hexpand: true,

                gtk::Label {
                    #[watch]
                    set_label: &self.get_strength_icon(),
                    set_width_request: 32,
                },

                gtk::Label {
                    #[watch]
                    set_label: &self.access_point.ssid,
                },

                gtk::Label {
                    add_css_class: "dim",
                    add_css_class: "access-point-frequency",
                    #[watch]
                    set_label: &format!("{:.1} GHz", self.access_point.frequency as f64 / 1000.0),
                    set_hexpand: true,
                    set_halign: gtk::Align::Start,
                },
            },
        }
    }

    fn init_model(init: Self::Init, _index: &DynamicIndex, _sender: FactorySender<Self>) -> Self {
        Self { access_point: init }
    }

    fn update(&mut self, msg: Self::Input, sender: FactorySender<Self>) {
        match msg {
            AccessPointMsg::Connect => {
                if self.access_point.requires_password {
                    let _ = sender.output(AccessPointOutput::RequestPassword(
                        self.access_point.ssid.clone(),
                    ));
                } else {
                    let _ =
                        sender.output(AccessPointOutput::Connect(self.access_point.ssid.clone()));
                }
            }
        }
    }
}

impl AccessPointWidget {
    fn get_strength_icon(&self) -> String {
        use crate::utils::icons::NETWORK_WIFI_ICONS;

        let icons = NETWORK_WIFI_ICONS;
        let strength_ratio = self.access_point.strength as f64 / 100.0;
        let index = (strength_ratio * (icons.len() - 1) as f64).round() as usize;

        icons.get(index).unwrap_or(&"󰤟").to_string()
    }
}

pub fn create_wifi_menu(initial_state: WifiState) -> gtk4::Widget {
    let controller = WiFiMenu::builder().launch(initial_state).detach();
    controller.widget().clone().into()
}
