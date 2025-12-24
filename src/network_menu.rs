use gtk4::prelude::*;
use relm4::prelude::*;

use crate::network::{NETWORK_STATE, NetworkInfo, dbus::AccessPointProxy, get_icon};

#[derive(Debug)]
pub struct NetworkMenu {
    network_state: NetworkInfo,
    show_password_dialog: Option<String>, // SSID requiring password
    access_points: AsyncFactoryVecDeque<AccessPointWidget>,
    scanning: bool,
}

#[derive(Debug)]
pub enum NetworkMenuMsg {
    ToggleWifi(bool),
    ScanNetworks,
    ConnectToNetwork(String),   // SSID
    ShowPasswordDialog(String), // SSID
    HidePasswordDialog,
    ConnectWithPassword(String, String), // SSID, Password
    UpdateState(NetworkInfo),
}

#[relm4::component(pub)]
impl SimpleComponent for NetworkMenu {
    type Init = ();
    type Input = NetworkMenuMsg;
    type Output = ();

    view! {
        #[root]
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 16,
            set_vexpand: true,
            set_width_request: 256,

            // header with wifi toggle
            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 20,
                set_hexpand: true,
                add_css_class: "content-title",

                gtk::Image {
                    #[watch]
                    set_icon_name: Some(get_icon(&model.network_state)),
                    set_icon_size: gtk4::IconSize::Large,
                    set_hexpand: true,
                    set_halign: gtk::Align::Start,
                },

                gtk::Switch {
                    #[watch]
                    set_active: !model.network_state.is_asleep(),
                    set_halign: gtk::Align::End,
                    set_valign: gtk::Align::End,

                    connect_state_set[sender] => move |_, state| {
                        sender.input(NetworkMenuMsg::ToggleWifi(state));
                        glib::Propagation::Proceed
                    },
                },
            },

            // scrollable content
            gtk::ScrolledWindow {
                set_vscrollbar_policy: gtk::PolicyType::Automatic,
                set_hscrollbar_policy: gtk::PolicyType::Never,
                set_vexpand: true,

                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 16,

                    // status information
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 4,

                        gtk::Label {
                            set_halign: gtk::Align::Start,
                            #[watch]
                            set_visible: model.network_state.wifi_ssid().is_some(),
                            #[watch]
                            set_label: &model.network_state.wifi_ssid()
                                .map(|ssid| format!("Connected to {}", ssid))
                                .unwrap_or_default(),
                        },

                        gtk::Label {
                            set_halign: gtk::Align::Start,
                            #[watch]
                            set_label: &model.network_state.connectivity.to_string(),
                        },

                        gtk::Label {
                            set_halign: gtk::Align::Start,
                            #[watch]
                            set_label: &model.network_state.connection_state.to_string(),
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
                                        sender.input(NetworkMenuMsg::HidePasswordDialog);
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
                        }
                    },
                },
            },
        }
    }

    fn init(
        _init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let access_points = AsyncFactoryVecDeque::builder()
            .launch(gtk::Box::default())
            .forward(sender.input_sender(), |output| match output {
                AccessPointOutput::Connect(ssid) => NetworkMenuMsg::ConnectToNetwork(ssid),
                AccessPointOutput::RequestPassword(ssid) => {
                    NetworkMenuMsg::ShowPasswordDialog(ssid)
                }
            });

        NETWORK_STATE.subscribe(sender.input_sender(), |state| {
            NetworkMenuMsg::UpdateState(state.clone())
        });

        let current_state = NETWORK_STATE.read().clone();

        let model = NetworkMenu {
            network_state: current_state,
            show_password_dialog: None,
            access_points,
            scanning: false,
        };

        // let access_points_box = model.access_points.widget();
        let widgets = view_output!();

        // // setup connect button click handler
        // widgets.connect_button.connect_clicked({
        //     let sender = sender.clone();
        //     let password_entry = widgets.password_entry.clone();
        //     move |_| {
        //         let password = password_entry.text().to_string();
        //         // get SSID from the dialog state (simplified for now)
        //         sender.input(NetworkMenuMsg::ConnectWithPassword(
        //             "current_ssid".to_string(),
        //             password,
        //         ));
        //     }
        // });

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            NetworkMenuMsg::UpdateState(state) => {
                self.network_state = state;
            }
            NetworkMenuMsg::ToggleWifi(enabled) => {
                // TODO: implement actual wifi toggle
                todo!("toggle wifi: {}", enabled);
            }
            NetworkMenuMsg::ScanNetworks => {
                // TODO: implement network scan
                todo!("scanning for networks");
            }
            NetworkMenuMsg::ConnectToNetwork(ssid) => {
                // TODO: implement connection logic
                todo!("connecting to network: {}", ssid);
            }
            NetworkMenuMsg::ShowPasswordDialog(ssid) => {
                self.show_password_dialog = Some(ssid);
            }
            NetworkMenuMsg::HidePasswordDialog => {
                self.show_password_dialog = None;
            }
            NetworkMenuMsg::ConnectWithPassword(ssid, _password) => {
                // TODO: implement password connection
                self.show_password_dialog = None;
                todo!("connecting to {} with password", ssid);
            }
        }
    }
}

// factory for individual access point items
#[derive(Debug)]
struct AccessPointWidget {
    access_point_proxy: AccessPointProxy<'static>,
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

pub struct AccessPointWidgetWidgets {
    main_box: gtk::Box,
    strength_icon: gtk::Image,
    ssid_label: gtk::Label,
    frequency_label: gtk::Label,
}

impl AsyncFactoryComponent for AccessPointWidget {
    type CommandOutput = ();
    type Init = AccessPointProxy<'static>;
    type Input = AccessPointMsg;
    type Output = AccessPointOutput;
    type ParentWidget = gtk::Box;
    type Root = gtk::Button;
    type Widgets = AccessPointWidgetWidgets;

    async fn init_model(
        init: Self::Init,
        _index: &DynamicIndex,
        _sender: AsyncFactorySender<Self>,
    ) -> Self {
        Self {
            access_point_proxy: init,
        }
    }

    async fn update(&mut self, msg: Self::Input, sender: AsyncFactorySender<Self>) {
        match msg {
            AccessPointMsg::Connect => {
                if let Ok(ssid_bytes) = self.access_point_proxy.ssid().await {
                    let ssid = String::from_utf8_lossy(&ssid_bytes).to_string();
                    let _ = sender.output(AccessPointOutput::RequestPassword(ssid));
                }
            }
        }
    }

    fn init_root() -> Self::Root {
        gtk::Button::builder().build()
    }

    fn init_widgets(
        &mut self,
        _index: &DynamicIndex,
        root: Self::Root,
        _returned_widget: &<Self::ParentWidget as relm4::factory::FactoryView>::ReturnedWidget,
        sender: AsyncFactorySender<Self>,
    ) -> Self::Widgets {
        root.connect_clicked(move |_| sender.input(AccessPointMsg::Connect));

        let main_box = gtk::Box::builder()
            .spacing(8)
            .halign(gtk::Align::Start)
            .hexpand(true)
            .build();

        let strength_icon = gtk::Image::builder().width_request(32).build();

        let ssid_label = gtk::Label::new(None);

        let frequency_label = gtk::Label::builder()
            .css_classes(["dim", "access-point-frequency"])
            .build();

        main_box.append(&strength_icon);
        main_box.append(&ssid_label);
        main_box.append(&frequency_label);

        AccessPointWidgetWidgets {
            main_box,
            strength_icon,
            ssid_label,
            frequency_label,
        }
    }
}
