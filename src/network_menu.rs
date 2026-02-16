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

pub struct NetworkMenuWidgets {
    wifi_icon: gtk::Image,
    wifi_switch: gtk::Switch,
    ssid_label: gtk::Label,
    connectivity_label: gtk::Label,
    connection_state_label: gtk::Label,
    password_dialog_box: gtk::Box,
    password_dialog_label: gtk::Label,
    password_entry: gtk::Entry,
    connect_button: gtk::Button,
}

impl SimpleComponent for NetworkMenu {
    type Init = ();
    type Input = NetworkMenuMsg;
    type Output = ();
    type Root = gtk::Box;
    type Widgets = NetworkMenuWidgets;

    fn init_root() -> Self::Root {
        gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(16)
            .vexpand(true)
            .width_request(256)
            .build()
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

        // header with wifi toggle
        let header_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(20)
            .hexpand(true)
            .build();
        header_box.add_css_class("content-title");

        let wifi_icon = gtk::Image::builder()
            .icon_name(get_icon(&current_state))
            .icon_size(gtk4::IconSize::Large)
            .hexpand(true)
            .halign(gtk::Align::Start)
            .build();

        let wifi_switch = gtk::Switch::builder()
            .active(!current_state.is_asleep())
            .halign(gtk::Align::End)
            .valign(gtk::Align::End)
            .build();

        wifi_switch.connect_state_set({
            let sender = sender.clone();
            move |_, state| {
                sender.input(NetworkMenuMsg::ToggleWifi(state));
                glib::Propagation::Proceed
            }
        });

        header_box.append(&wifi_icon);
        header_box.append(&wifi_switch);

        // scrollable content
        let scrolled_window = gtk::ScrolledWindow::builder()
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .build();

        let content_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(16)
            .build();

        // status information
        let status_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(4)
            .build();

        let ssid_label = gtk::Label::builder()
            .halign(gtk::Align::Start)
            .visible(current_state.wifi_ssid().is_some())
            .label(
                &current_state
                    .wifi_ssid()
                    .map(|ssid| format!("Connected to {}", ssid))
                    .unwrap_or_default(),
            )
            .build();

        let connectivity_label = gtk::Label::builder()
            .halign(gtk::Align::Start)
            .label(&current_state.connectivity.to_string())
            .build();

        let connection_state_label = gtk::Label::builder()
            .halign(gtk::Align::Start)
            .label(&current_state.connection_state.to_string())
            .build();

        status_box.append(&ssid_label);
        status_box.append(&connectivity_label);
        status_box.append(&connection_state_label);

        // password dialog box (initially hidden)
        let password_dialog_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(8)
            .visible(false)
            .build();

        let password_dialog_label = gtk::Label::new(None);

        let password_entry = gtk::Entry::builder()
            .visibility(false)
            .placeholder_text("Password")
            .build();

        let dialog_buttons_box = gtk::Box::builder().spacing(8).build();

        let cancel_button = gtk::Button::builder().label("Cancel").build();
        cancel_button.connect_clicked({
            let sender = sender.clone();
            move |_| {
                sender.input(NetworkMenuMsg::HidePasswordDialog);
            }
        });

        let connect_button = gtk::Button::builder().label("Connect").build();
        connect_button.connect_clicked({
            let sender = sender.clone();
            let password_entry = password_entry.clone();
            move |_| {
                let password = password_entry.text().to_string();
                // TODO: need to get SSID from model state
                sender.input(NetworkMenuMsg::ConnectWithPassword(
                    "current_ssid".to_string(),
                    password,
                ));
            }
        });

        dialog_buttons_box.append(&cancel_button);
        dialog_buttons_box.append(&connect_button);

        password_dialog_box.append(&password_dialog_label);
        password_dialog_box.append(&password_entry);
        password_dialog_box.append(&dialog_buttons_box);

        content_box.append(&status_box);
        content_box.append(&password_dialog_box);

        scrolled_window.set_child(Some(&content_box));

        root.append(&header_box);
        root.append(&scrolled_window);

        let model = NetworkMenu {
            network_state: current_state,
            show_password_dialog: None,
            access_points,
            scanning: false,
        };

        let widgets = NetworkMenuWidgets {
            wifi_icon,
            wifi_switch,
            ssid_label,
            connectivity_label,
            connection_state_label,
            password_dialog_box,
            password_dialog_label,
            password_entry,
            connect_button,
        };

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

    fn update_view(&self, widgets: &mut Self::Widgets, _sender: ComponentSender<Self>) {
        widgets
            .wifi_icon
            .set_icon_name(Some(get_icon(&self.network_state)));
        widgets
            .wifi_switch
            .set_active(!self.network_state.is_asleep());

        widgets
            .ssid_label
            .set_visible(self.network_state.wifi_ssid().is_some());
        widgets.ssid_label.set_label(
            &self
                .network_state
                .wifi_ssid()
                .map(|ssid| format!("Connected to {}", ssid))
                .unwrap_or_default(),
        );

        widgets
            .connectivity_label
            .set_label(&self.network_state.connectivity.to_string());
        widgets
            .connection_state_label
            .set_label(&self.network_state.connection_state.to_string());

        // update password dialog visibility
        if let Some(ssid) = &self.show_password_dialog {
            widgets.password_dialog_box.set_visible(true);
            widgets
                .password_dialog_label
                .set_label(&format!("Enter password for {}", ssid));
        } else {
            widgets.password_dialog_box.set_visible(false);
            widgets.password_entry.set_text("");
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
