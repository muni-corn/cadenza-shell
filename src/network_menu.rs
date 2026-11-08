use std::time::Duration;

use gtk4::prelude::*;
use relm4::prelude::*;
use zbus::zvariant::OwnedObjectPath;

use crate::{
    network::{
        self, AccessPointSummary, ApSecurity, ConnectFailureReason, NETWORK_STATE, NetworkEvent,
        NetworkInfo, WIFI_SCAN_STATE, WifiScanState, get_icon, get_strength_icon, subscribe_events,
    },
    settings,
};

#[derive(Debug)]
pub struct NetworkMenu {
    network_state: NetworkInfo,
    scan_state: WifiScanState,
    access_points: FactoryVecDeque<AccessPointRow>,
    password_prompt: Option<PasswordPrompt>,
    connect_error: Option<String>,
    /// Whether the menu's containing window is currently open, set by the
    /// owning tile via [`NetworkMenuMsg::SetOpen`]. Gates the periodic
    /// rescan so we don't poke the radio while nobody's looking.
    is_open: bool,
    /// Bumped on every open and close so rescan ticks from an earlier open
    /// period are ignored.
    rescan_generation: u64,
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
    /// The profile whose secrets were rejected, replaced on submit.
    saved_connection: Option<OwnedObjectPath>,
}

#[derive(Debug)]
pub enum NetworkMenuMsg {
    ToggleWifi(bool),
    Rescan,
    Disconnect,
    ApClicked(AccessPointSummary),
    ForgetClicked(AccessPointSummary),
    PasswordSubmitted(String),
    CancelPassword,
    UpdateState(NetworkInfo),
    UpdateScanState(WifiScanState),
    /// Sent by the owning tile when its menu window opens or closes.
    SetOpen(bool),
}

#[derive(Debug)]
pub struct NetworkMenuWidgets {
    wifi_icon: gtk::Image,
    wifi_switch: gtk::Switch,
    wifi_switch_handler: glib::SignalHandlerId,
    /// The `wifi_enabled` value last pushed into the switch, so it's only
    /// resynced when the real state changes rather than after every message.
    synced_wifi_enabled: bool,
    ssid_label: gtk::Label,
    connectivity_label: gtk::Label,
    connection_state_label: gtk::Label,
    disconnect_button: gtk::Button,
    rescan_button: gtk::Button,
    scanning_label: gtk::Label,
    error_label: gtk::Label,
    password_box: gtk::Box,
    password_label: gtk::Label,
    password_entry: gtk::Entry,
}

impl Component for NetworkMenu {
    type CommandOutput = NetworkMenuCommand;
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
            .width_request(320)
            .height_request(512)
            .build()
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
                    .spacing(4)
                    .build(),
            )
            .forward(sender.input_sender(), |output| match output {
                AccessPointRowOutput::Clicked(summary) => NetworkMenuMsg::ApClicked(summary),
                AccessPointRowOutput::ForgetClicked(summary) => {
                    NetworkMenuMsg::ForgetClicked(summary)
                }
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
            .active(current_state.wifi_enabled)
            .halign(gtk::Align::End)
            .valign(gtk::Align::End)
            .build();

        let wifi_switch_handler = wifi_switch.connect_state_set({
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

        // password prompt (initially hidden)
        let password_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(8)
            .visible(false)
            .build();

        let password_label = gtk::Label::builder().halign(gtk::Align::Start).build();

        let password_entry = gtk::Entry::builder()
            .visibility(false)
            .placeholder_text("Password")
            .activates_default(true)
            .build();

        let dialog_buttons_box = gtk::Box::builder().spacing(8).build();

        let cancel_button = gtk::Button::builder().label("Cancel").build();
        cancel_button.connect_clicked({
            let sender = sender.clone();
            move |_| sender.input(NetworkMenuMsg::CancelPassword)
        });

        let connect_button = gtk::Button::builder().label("Connect").build();
        let submit = {
            let sender = sender.clone();
            let password_entry = password_entry.clone();
            move || {
                sender.input(NetworkMenuMsg::PasswordSubmitted(
                    password_entry.text().to_string(),
                ));
            }
        };
        connect_button.connect_clicked({
            let submit = submit.clone();
            move |_| submit()
        });
        password_entry.connect_activate(move |_| submit());

        dialog_buttons_box.append(&cancel_button);
        dialog_buttons_box.append(&connect_button);

        password_box.append(&password_label);
        password_box.append(&password_entry);
        password_box.append(&dialog_buttons_box);

        content_box.append(&status_box);
        content_box.append(&rescan_row);
        content_box.append(&error_label);
        content_box.append(&password_box);
        content_box.append(access_points.widget());

        scrolled_window.set_child(Some(&content_box));

        root.append(&header_box);
        root.append(&scrolled_window);

        let mut model = NetworkMenu {
            network_state: current_state,
            scan_state: WifiScanState::default(),
            access_points,
            password_prompt: None,
            connect_error: None,
            is_open: false,
            rescan_generation: 0,
        };
        update_ap_rows(
            &mut model.access_points,
            &current_scan_state.access_points,
            model.network_state.wifi_ssid().as_deref(),
        );
        model.scan_state = current_scan_state;

        let widgets = NetworkMenuWidgets {
            wifi_icon,
            wifi_switch,
            wifi_switch_handler,
            synced_wifi_enabled: model.network_state.wifi_enabled,
            ssid_label,
            connectivity_label,
            connection_state_label,
            disconnect_button,
            rescan_button,
            scanning_label,
            error_label,
            password_box,
            password_label,
            password_entry,
        };

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>, _root: &Self::Root) {
        match msg {
            NetworkMenuMsg::UpdateState(state) => {
                self.network_state = state;
                update_ap_rows(
                    &mut self.access_points,
                    &self.scan_state.access_points,
                    self.network_state.wifi_ssid().as_deref(),
                );
            }
            NetworkMenuMsg::UpdateScanState(state) => {
                update_ap_rows(
                    &mut self.access_points,
                    &state.access_points,
                    self.network_state.wifi_ssid().as_deref(),
                );
                self.scan_state = state;
            }
            NetworkMenuMsg::ToggleWifi(enabled) => {
                network::set_wifi_enabled(enabled);
            }
            NetworkMenuMsg::Rescan => {
                network::request_scan();
            }
            NetworkMenuMsg::SetOpen(open) => {
                self.is_open = open;
                self.rescan_generation = self.rescan_generation.wrapping_add(1);
                if open {
                    // fetch fresh results right away instead of waiting for
                    // the next periodic tick
                    network::request_scan();
                    self.schedule_rescan(&sender);
                }
            }
            NetworkMenuMsg::Disconnect => {
                network::disconnect();
            }
            NetworkMenuMsg::ApClicked(summary) => {
                self.connect_error = None;
                if self.network_state.wifi_ssid().as_deref() == Some(summary.ssid.as_str()) {
                    // already connected to this network; nothing to do here,
                    // the Disconnect button handles leaving it
                } else if summary.security == ApSecurity::Enterprise && !summary.is_saved() {
                    self.connect_error =
                        Some("Enterprise networks aren't supported here".to_string());
                } else if summary.needs_password() {
                    self.password_prompt = Some(PasswordPrompt {
                        ssid: summary.ssid,
                        security: summary.security,
                        saved_connection: None,
                    });
                } else {
                    network::connect(
                        summary.ssid,
                        summary.security,
                        None,
                        summary.saved_connection,
                    );
                }
            }
            NetworkMenuMsg::ForgetClicked(summary) => {
                if let Some(connection_path) = summary.saved_connection {
                    network::forget(connection_path);
                }
            }
            NetworkMenuMsg::PasswordSubmitted(password) => {
                // keep the prompt open so the password can be corrected
                if let Some(prompt) = &self.password_prompt
                    && let Err(problem) = prompt.security.validate_password(&password)
                {
                    self.connect_error = Some(problem.to_string());
                    return;
                }

                self.connect_error = None;
                if let Some(prompt) = self.password_prompt.take() {
                    network::connect(
                        prompt.ssid,
                        prompt.security,
                        Some(password),
                        prompt.saved_connection,
                    );
                }
            }
            NetworkMenuMsg::CancelPassword => {
                self.password_prompt = None;
                self.connect_error = None;
            }
        }
    }

    fn update_cmd(
        &mut self,
        command: Self::CommandOutput,
        sender: ComponentSender<Self>,
        _root: &Self::Root,
    ) {
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
    }

    fn update_view(&self, widgets: &mut Self::Widgets, _sender: ComponentSender<Self>) {
        widgets
            .wifi_icon
            .set_icon_name(Some(get_icon(&self.network_state)));

        // set_active emits state-set, which would send ToggleWifi again
        if widgets.synced_wifi_enabled != self.network_state.wifi_enabled {
            widgets.synced_wifi_enabled = self.network_state.wifi_enabled;
            widgets
                .wifi_switch
                .block_signal(&widgets.wifi_switch_handler);
            widgets
                .wifi_switch
                .set_active(self.network_state.wifi_enabled);
            widgets
                .wifi_switch
                .unblock_signal(&widgets.wifi_switch_handler);
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

        if let Some(prompt) = &self.password_prompt {
            widgets.password_box.set_visible(true);
            widgets
                .password_label
                .set_label(&format!("Enter password for {}", prompt.ssid));
        } else {
            widgets.password_box.set_visible(false);
            widgets.password_entry.set_text("");
        }
    }
}

impl NetworkMenu {
    /// Requests another scan after `network.scan_interval` while the menu
    /// stays open. RequestScan only starts a scan, so repeating it is what
    /// eventually surfaces fresh results.
    fn schedule_rescan(&self, sender: &ComponentSender<Self>) {
        let generation = self.rescan_generation;
        let interval = Duration::from_secs(settings::get_config().network.scan_interval);
        sender.oneshot_command(async move {
            tokio::time::sleep(interval).await;
            NetworkMenuCommand::RescanTick(generation)
        });
    }

    fn handle_connection_event(&mut self, event: NetworkEvent) {
        match event {
            NetworkEvent::ConnectionSucceeded { ssid } => {
                tracing::debug!("connected to {ssid}");
                self.password_prompt = None;
                self.connect_error = None;
            }
            NetworkEvent::ConnectionFailed { ssid, reason } => {
                self.connect_error = Some(match reason {
                    ConnectFailureReason::WrongPassword => {
                        format!("Wrong password for {ssid}")
                    }
                    ConnectFailureReason::Other => format!("Couldn't connect to {ssid}"),
                });

                // reopen the password prompt so the user can retry,
                // rather than leaving them nowhere after a wrong
                // password
                if reason == ConnectFailureReason::WrongPassword {
                    let network = self
                        .scan_state
                        .access_points
                        .iter()
                        .find(|ap| ap.ssid == ssid)
                        .map(|ap| (ap.security, ap.saved_connection.clone()));
                    if let Some((security, saved_connection)) = network {
                        self.password_prompt = Some(PasswordPrompt {
                            ssid,
                            security,
                            saved_connection,
                        });
                    }
                }
            }
        }
    }
}

/// Syncs the factory's rows with the latest scan results, with the connected
/// network first and the rest by descending strength.
///
/// Rows are updated in place when the order is unchanged, which is the
/// common case between scans; otherwise the list is rebuilt so the order is
/// always right.
fn update_ap_rows(
    rows: &mut FactoryVecDeque<AccessPointRow>,
    summaries: &[AccessPointSummary],
    active_ssid: Option<&str>,
) {
    let mut ordered: Vec<(AccessPointSummary, bool)> = summaries
        .iter()
        .map(|summary| (summary.clone(), Some(summary.ssid.as_str()) == active_ssid))
        .collect();
    // stable, so the strength order from the scan is kept
    ordered.sort_by_key(|(_, active)| !active);

    let same_order = rows.len() == ordered.len()
        && rows
            .iter()
            .zip(&ordered)
            .all(|(row, (summary, _))| row.summary.ssid == summary.ssid);

    if same_order {
        for (index, (summary, active)) in ordered.into_iter().enumerate() {
            rows.send(index, AccessPointRowMsg::Update(summary, active));
        }
    } else {
        let mut guard = rows.guard();
        guard.clear();
        for row in ordered {
            guard.push_back(row);
        }
    }
}

// factory for individual access point rows
#[derive(Debug)]
struct AccessPointRow {
    summary: AccessPointSummary,
    /// Whether the wifi device is connected to this network.
    active: bool,
}

#[derive(Debug)]
enum AccessPointRowMsg {
    Clicked,
    ForgetClicked,
    Update(AccessPointSummary, bool),
}

#[derive(Debug)]
enum AccessPointRowOutput {
    Clicked(AccessPointSummary),
    ForgetClicked(AccessPointSummary),
}

struct AccessPointRowWidgets {
    strength_icon: gtk::Image,
    ssid_label: gtk::Label,
    status_label: gtk::Label,
    forget_button: gtk::Button,
}

impl FactoryComponent for AccessPointRow {
    type CommandOutput = ();
    type Index = DynamicIndex;
    /// The network and whether it's the connected one.
    type Init = (AccessPointSummary, bool);
    type Input = AccessPointRowMsg;
    type Output = AccessPointRowOutput;
    type ParentWidget = gtk::Box;
    type Root = gtk::Box;
    type Widgets = AccessPointRowWidgets;

    fn init_model(
        (summary, active): Self::Init,
        _index: &DynamicIndex,
        _sender: FactorySender<Self>,
    ) -> Self {
        Self { summary, active }
    }

    fn init_root(&self) -> Self::Root {
        gtk::Box::builder().spacing(4).build()
    }

    fn init_widgets(
        &mut self,
        _index: &DynamicIndex,
        root: Self::Root,
        _returned_widget: &<Self::ParentWidget as relm4::factory::FactoryView>::ReturnedWidget,
        sender: FactorySender<Self>,
    ) -> Self::Widgets {
        let select_button = gtk::Button::builder().hexpand(true).build();
        select_button.connect_clicked({
            let sender = sender.clone();
            move |_| sender.input(AccessPointRowMsg::Clicked)
        });

        let main_box = gtk::Box::builder()
            .spacing(8)
            .halign(gtk::Align::Start)
            .hexpand(true)
            .build();

        let strength_icon = gtk::Image::builder().width_request(24).build();
        let ssid_label = gtk::Label::builder()
            .halign(gtk::Align::Start)
            .hexpand(true)
            .build();
        let status_label = gtk::Label::builder()
            .css_classes(["dim"])
            .halign(gtk::Align::End)
            .build();

        main_box.append(&strength_icon);
        main_box.append(&ssid_label);
        main_box.append(&status_label);

        select_button.set_child(Some(&main_box));

        let forget_button = gtk::Button::builder()
            .icon_name("user-trash-symbolic")
            .tooltip_text("Forget this network")
            .visible(self.summary.is_saved())
            .build();
        forget_button.connect_clicked(move |_| sender.input(AccessPointRowMsg::ForgetClicked));

        root.append(&select_button);
        root.append(&forget_button);

        let widgets = AccessPointRowWidgets {
            strength_icon,
            ssid_label,
            status_label,
            forget_button,
        };
        self.render(&widgets);
        widgets
    }

    fn update(&mut self, msg: Self::Input, sender: FactorySender<Self>) {
        match msg {
            AccessPointRowMsg::Clicked => {
                let _ = sender.output(AccessPointRowOutput::Clicked(self.summary.clone()));
            }
            AccessPointRowMsg::ForgetClicked => {
                let _ = sender.output(AccessPointRowOutput::ForgetClicked(self.summary.clone()));
            }
            AccessPointRowMsg::Update(summary, active) => {
                self.summary = summary;
                self.active = active;
            }
        }
    }

    fn update_view(&self, widgets: &mut Self::Widgets, _sender: FactorySender<Self>) {
        self.render(widgets);
    }
}

impl AccessPointRow {
    fn render(&self, widgets: &AccessPointRowWidgets) {
        widgets
            .strength_icon
            .set_icon_name(Some(get_strength_icon(self.summary.strength)));
        widgets.ssid_label.set_label(&self.summary.ssid);

        let status = if self.active {
            "Connected"
        } else if self.summary.is_saved() {
            "Saved"
        } else if !self.summary.security.is_open() {
            "Secured"
        } else {
            ""
        };
        widgets.status_label.set_visible(!status.is_empty());
        widgets.status_label.set_label(status);

        widgets.forget_button.set_visible(self.summary.is_saved());
    }
}
