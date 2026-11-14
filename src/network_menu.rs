mod handlers;
mod rows;

use gtk4::prelude::*;
use relm4::prelude::*;
use zbus::zvariant::OwnedObjectPath;

use self::rows::update_ap_rows;
use crate::{
    network::{
        self, ApSecurity, NETWORK_STATE, NetworkEvent, NetworkInfo, Password, WIFI_SCAN_STATE,
        WifiScanState, get_icon, subscribe_events,
    },
    widgets::{
        menu_parts::{MenuHeader, PromptBox, menu_root, scrolled_content},
        menu_row::{MenuRow, MenuRowOutput},
    },
};

#[derive(Debug)]
pub struct NetworkMenu {
    network_state: NetworkInfo,
    scan_state: WifiScanState,
    access_points: FactoryVecDeque<MenuRow>,
    password_prompt: Option<PasswordPrompt>,
    connect_error: Option<String>,
    /// Whether the menu's containing window is currently open, set by the
    /// owning tile via [`NetworkMenuMsg::SetOpen`]. Gates the periodic
    /// rescan so we don't poke the radio while nobody's looking.
    is_open: bool,
    /// Bumped on every open and close so rescan ticks from an earlier open
    /// period are ignored.
    rescan_generation: u64,
    /// Set when new state arrives, so the switch only snaps to the real
    /// value then and not right after the user flips it.
    resync_wifi_switch: bool,
    /// Set when a password prompt opens, so the entry is cleared and
    /// focused.
    reset_password_entry: bool,
}

/// Results of the menu's background commands.
#[derive(Debug)]
pub enum NetworkMenuCommand {
    /// A connection attempt finished.
    ConnectionEvent(NetworkEvent),
    /// The rescan delay for the given open period elapsed.
    RescanTick(u64),
}

#[derive(Debug, Clone)]
struct PasswordPrompt {
    ssid: String,
    security: ApSecurity,
    /// Profiles whose secrets were rejected, replaced on submit.
    saved_connections: Vec<OwnedObjectPath>,
}

#[derive(Debug)]
pub enum NetworkMenuMsg {
    ToggleWifi(bool),
    Rescan,
    Disconnect,
    /// A network's row was clicked, identified by its ssid.
    ApClicked(String),
    /// A network's forget button was clicked, identified by its ssid.
    ForgetClicked(String),
    PasswordSubmitted(Password),
    CancelPassword,
    UpdateState(NetworkInfo),
    UpdateScanState(WifiScanState),
    /// Sent by the owning tile when its menu window opens or closes.
    SetOpen(bool),
}

#[derive(Debug)]
pub struct NetworkMenuWidgets {
    header: MenuHeader,
    ssid_label: gtk::Label,
    connectivity_label: gtk::Label,
    connection_state_label: gtk::Label,
    disconnect_button: gtk::Button,
    rescan_button: gtk::Button,
    scanning_label: gtk::Label,
    error_label: gtk::Label,
    password_prompt: PromptBox,
}

impl Component for NetworkMenu {
    type CommandOutput = NetworkMenuCommand;
    type Init = ();
    type Input = NetworkMenuMsg;
    type Output = ();
    type Root = gtk::Box;
    type Widgets = NetworkMenuWidgets;

    fn init_root() -> Self::Root {
        menu_root()
    }

    fn init(
        _init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let access_points = FactoryVecDeque::builder()
            .launch(
                gtk::Box::builder()
                    .orientation(gtk::Orientation::Vertical)
                    .spacing(8)
                    .build(),
            )
            .forward(sender.input_sender(), |output| match output {
                MenuRowOutput::Activated(ssid) => NetworkMenuMsg::ApClicked(ssid),
                MenuRowOutput::Remove(ssid) => NetworkMenuMsg::ForgetClicked(ssid),
            });

        NETWORK_STATE.subscribe(sender.input_sender(), |state| {
            NetworkMenuMsg::UpdateState(state.clone())
        });
        WIFI_SCAN_STATE.subscribe(sender.input_sender(), |state| {
            NetworkMenuMsg::UpdateScanState(state.clone())
        });

        // forward connection attempt outcomes; the command is cancelled when
        // the menu shuts down
        sender.command(|out, shutdown| {
            shutdown
                .register(async move {
                    let mut rx = subscribe_events();
                    loop {
                        match rx.recv().await {
                            Ok(event) => {
                                if out
                                    .send(NetworkMenuCommand::ConnectionEvent(event))
                                    .is_err()
                                {
                                    break;
                                }
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                                tracing::warn!(
                                    "network menu lagged on connection event broadcast, missed \
                                     {n} event(s)"
                                );
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                        }
                    }
                })
                .drop_on_shutdown()
        });

        let current_state = NETWORK_STATE.read().clone();
        let current_scan_state = WIFI_SCAN_STATE.read().clone();

        let header = MenuHeader::new(get_icon(&current_state), current_state.wifi_enabled, {
            let sender = sender.clone();
            move |enabled| sender.input(NetworkMenuMsg::ToggleWifi(enabled))
        });
        let (scrolled_window, content_box) = scrolled_content();

        // status information
        let status_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(4)
            .build();

        let ssid_row = gtk::Box::builder().spacing(8).build();

        let ssid_label = gtk::Label::builder()
            .halign(gtk::Align::Start)
            .hexpand(true)
            .visible(current_state.wifi_ssid().is_some())
            .label(
                current_state
                    .wifi_ssid()
                    .map(|ssid| format!("Connected to {ssid}"))
                    .unwrap_or_default(),
            )
            .build();

        let disconnect_button = gtk::Button::builder()
            .label("Disconnect")
            .visible(current_state.wifi_ssid().is_some())
            .build();
        disconnect_button.connect_clicked({
            let sender = sender.clone();
            move |_| sender.input(NetworkMenuMsg::Disconnect)
        });

        ssid_row.append(&ssid_label);
        ssid_row.append(&disconnect_button);

        let connectivity_label = gtk::Label::builder()
            .halign(gtk::Align::Start)
            .label(current_state.connectivity.to_string())
            .build();

        let connection_state_label = gtk::Label::builder()
            .halign(gtk::Align::Start)
            .label(current_state.connection_state.to_string())
            .build();

        status_box.append(&ssid_row);
        status_box.append(&connectivity_label);
        status_box.append(&connection_state_label);

        // rescan row
        let rescan_row = gtk::Box::builder().spacing(8).build();

        let rescan_button = gtk::Button::builder().label("Rescan").build();
        rescan_button.connect_clicked({
            let sender = sender.clone();
            move |_| sender.input(NetworkMenuMsg::Rescan)
        });

        let scanning_label = gtk::Label::builder()
            .label("Scanning...")
            .css_classes(["dim"])
            .visible(current_scan_state.scanning)
            .build();

        rescan_row.append(&rescan_button);
        rescan_row.append(&scanning_label);

        // connection error banner (covers both the "retry password" case and
        // a plain failed connect to a saved/open network)
        let error_label = gtk::Label::builder()
            .halign(gtk::Align::Start)
            .css_classes(["warning"])
            .visible(false)
            .wrap(true)
            .build();

        let password_prompt = PromptBox::new(
            "Connect",
            "Password",
            true,
            {
                let sender = sender.clone();
                move |password| {
                    let password = Password::new(password.unwrap_or_default());
                    sender.input(NetworkMenuMsg::PasswordSubmitted(password));
                }
            },
            {
                let sender = sender.clone();
                move || sender.input(NetworkMenuMsg::CancelPassword)
            },
        );

        content_box.append(&status_box);
        content_box.append(&rescan_row);
        content_box.append(&error_label);
        content_box.append(&password_prompt.root);
        content_box.append(access_points.widget());

        root.append(&header.root);
        root.append(&scrolled_window);

        let mut model = NetworkMenu {
            network_state: current_state,
            scan_state: WifiScanState::default(),
            access_points,
            password_prompt: None,
            connect_error: None,
            is_open: false,
            rescan_generation: 0,
            resync_wifi_switch: false,
            reset_password_entry: false,
        };
        update_ap_rows(
            &mut model.access_points,
            &current_scan_state.access_points,
            model.network_state.wifi_ssid().as_deref(),
        );
        model.scan_state = current_scan_state;

        let widgets = NetworkMenuWidgets {
            header,
            ssid_label,
            connectivity_label,
            connection_state_label,
            disconnect_button,
            rescan_button,
            scanning_label,
            error_label,
            password_prompt,
        };

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>, _root: &Self::Root) {
        self.resync_wifi_switch = false;
        let had_prompt = self.password_prompt.is_some();
        self.update_message(msg, &sender);
        self.reset_password_entry = !had_prompt && self.password_prompt.is_some();
    }

    fn update_cmd(
        &mut self,
        command: Self::CommandOutput,
        sender: ComponentSender<Self>,
        _root: &Self::Root,
    ) {
        let had_prompt = self.password_prompt.is_some();
        match command {
            NetworkMenuCommand::ConnectionEvent(event) => self.handle_connection_event(event),
            NetworkMenuCommand::RescanTick(generation) => {
                // ticks from an earlier open period are stale
                if self.is_open && generation == self.rescan_generation {
                    network::request_scan();
                    self.schedule_rescan(&sender);
                }
            }
        }
        self.reset_password_entry = !had_prompt && self.password_prompt.is_some();
    }

    fn update_view(&self, widgets: &mut Self::Widgets, _sender: ComponentSender<Self>) {
        widgets.header.set_icon(get_icon(&self.network_state));
        if self.resync_wifi_switch {
            widgets.header.sync_switch(self.network_state.wifi_enabled);
        }

        let ssid = self.network_state.wifi_ssid();
        widgets.ssid_label.set_visible(ssid.is_some());
        widgets.disconnect_button.set_visible(ssid.is_some());
        widgets.ssid_label.set_label(
            &ssid
                .map(|ssid| format!("Connected to {ssid}"))
                .unwrap_or_default(),
        );

        widgets
            .connectivity_label
            .set_label(&self.network_state.connectivity.to_string());
        widgets
            .connection_state_label
            .set_label(&self.network_state.connection_state.to_string());

        widgets
            .rescan_button
            .set_sensitive(!self.scan_state.scanning);
        widgets.scanning_label.set_visible(self.scan_state.scanning);

        widgets
            .error_label
            .set_visible(self.connect_error.is_some());
        widgets
            .error_label
            .set_label(self.connect_error.as_deref().unwrap_or_default());

        match &self.password_prompt {
            Some(prompt) => {
                widgets.password_prompt.show(
                    &format!("Enter password for {}", prompt.ssid),
                    true,
                    true,
                );
                if self.reset_password_entry {
                    widgets.password_prompt.reset_entry();
                }
            }
            None => widgets.password_prompt.hide(),
        }
    }
}
