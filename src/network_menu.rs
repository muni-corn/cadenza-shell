use std::{collections::HashSet, time::Duration};

use gtk4::prelude::*;
use relm4::prelude::*;

use crate::{
    network::{
        self, AccessPointSummary, ConnectFailureReason, NETWORK_STATE, NetworkEvent, NetworkInfo,
        WIFI_SCAN_STATE, WifiScanState, get_icon, get_strength_icon, subscribe_events,
        types::ApSecurity,
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
}

#[derive(Debug, Clone)]
struct PasswordPrompt {
    ssid: String,
    security: ApSecurity,
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
    ConnectionEvent(NetworkEvent),
    /// Sent by the owning tile when its menu window opens or closes.
    SetOpen(bool),
    /// Internal: periodic tick requesting a rescan while open.
    PeriodicRescan,
}

#[derive(Debug)]
pub struct NetworkMenuWidgets {
    wifi_icon: gtk::Image,
    wifi_switch: gtk::Switch,
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

        // forward connection attempt outcomes into our own input channel
        let sender_clone = sender.clone();
        relm4::spawn(async move {
            let mut rx = subscribe_events();
            loop {
                match rx.recv().await {
                    Ok(event) => sender_clone.input(NetworkMenuMsg::ConnectionEvent(event)),
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(
                            "network menu lagged on connection event broadcast, missed {n} \
                             event(s)"
                        );
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });

        // periodically request a rescan while the menu is open; RequestScan
        // only starts a scan, it doesn't return fresh results itself, so
        // this is what eventually surfaces them once LastScan advances
        let sender_clone = sender.clone();
        relm4::spawn(async move {
            let interval_secs = settings::get_config().network.scan_interval;
            let mut interval = tokio::time::interval(Duration::from_secs(interval_secs));
            interval.tick().await; // skip the immediate first tick
            loop {
                interval.tick().await;
                sender_clone.input(NetworkMenuMsg::PeriodicRescan);
            }
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
        };
        update_ap_rows(&mut model.access_points, &current_scan_state.access_points);
        model.scan_state = current_scan_state;

        let widgets = NetworkMenuWidgets {
            wifi_icon,
            wifi_switch,
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

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            NetworkMenuMsg::UpdateState(state) => {
                self.network_state = state;
            }
            NetworkMenuMsg::UpdateScanState(state) => {
                update_ap_rows(&mut self.access_points, &state.access_points);
                self.scan_state = state;
            }
            NetworkMenuMsg::ToggleWifi(enabled) => {
                network::set_wifi_enabled(enabled);
            }
            NetworkMenuMsg::Rescan => {
                network::scan();
            }
            NetworkMenuMsg::SetOpen(open) => {
                self.is_open = open;
                if open {
                    // fetch fresh results right away instead of waiting for
                    // the next periodic tick
                    network::scan();
                }
            }
            NetworkMenuMsg::PeriodicRescan => {
                if self.is_open {
                    network::scan();
                }
            }
            NetworkMenuMsg::Disconnect => {
                network::disconnect();
            }
            NetworkMenuMsg::ApClicked(summary) => {
                self.connect_error = None;
                if summary.is_active {
                    // already connected to this network; nothing to do here,
                    // the Disconnect button handles leaving it
                } else if summary.needs_password() {
                    self.password_prompt = Some(PasswordPrompt {
                        ssid: summary.ssid,
                        security: summary.security,
                    });
                } else {
                    network::connect(summary.ssid, summary.security, None);
                }
            }
            NetworkMenuMsg::ForgetClicked(summary) => {
                if let Some(connection_path) = summary.saved_connection {
                    network::forget(connection_path);
                }
            }
            NetworkMenuMsg::PasswordSubmitted(password) => {
                if let Some(prompt) = self.password_prompt.take() {
                    network::connect(prompt.ssid, prompt.security, Some(password));
                }
            }
            NetworkMenuMsg::CancelPassword => {
                self.password_prompt = None;
                self.connect_error = None;
            }
            NetworkMenuMsg::ConnectionEvent(event) => match event {
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
                        let security = self
                            .scan_state
                            .access_points
                            .iter()
                            .find(|ap| ap.ssid == ssid)
                            .map(|ap| ap.security);
                        if let Some(security) = security {
                            self.password_prompt = Some(PasswordPrompt { ssid, security });
                        }
                    }
                }
            },
        }
    }

    fn update_view(&self, widgets: &mut Self::Widgets, _sender: ComponentSender<Self>) {
        widgets
            .wifi_icon
            .set_icon_name(Some(get_icon(&self.network_state)));
        widgets
            .wifi_switch
            .set_active(self.network_state.wifi_enabled);

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

/// Diffs the factory's current rows against the latest scan results: removes
/// rows for networks no longer in range, updates existing rows in place, and
/// inserts new ones - instead of clearing and rebuilding the whole list on
/// every scan (which would destroy and recreate every row's widgets even
/// when nothing about them changed).
///
/// Doesn't reorder existing rows to match a changed sort order (e.g. if a
/// network's signal strength overtakes another between scans); this is a
/// minor, self-correcting staleness rather than a correctness issue.
fn update_ap_rows(rows: &mut FactoryVecDeque<AccessPointRow>, summaries: &[AccessPointSummary]) {
    let new_ssids: HashSet<&str> = summaries.iter().map(|s| s.ssid.as_str()).collect();

    let current_ssids: Vec<String> = rows.iter().map(|r| r.summary.ssid.clone()).collect();
    {
        let mut guard = rows.guard();
        for (index, ssid) in current_ssids.iter().enumerate().rev() {
            if !new_ssids.contains(ssid.as_str()) {
                guard.remove(index);
            }
        }
    }

    for summary in summaries {
        let existing_index = rows.iter().position(|r| r.summary.ssid == summary.ssid);
        match existing_index {
            Some(index) => rows.send(index, AccessPointRowMsg::UpdateSummary(summary.clone())),
            None => {
                rows.guard().push_back(summary.clone());
            }
        }
    }
}

// factory for individual access point rows
#[derive(Debug)]
struct AccessPointRow {
    summary: AccessPointSummary,
}

#[derive(Debug)]
enum AccessPointRowMsg {
    Clicked,
    ForgetClicked,
    UpdateSummary(AccessPointSummary),
}

#[derive(Debug)]
enum AccessPointRowOutput {
    Clicked(AccessPointSummary),
    ForgetClicked(AccessPointSummary),
}

struct AccessPointRowWidgets {
    _main_box: gtk::Box,
    strength_icon: gtk::Image,
    ssid_label: gtk::Label,
    status_label: gtk::Label,
    forget_button: gtk::Button,
}

impl FactoryComponent for AccessPointRow {
    type CommandOutput = ();
    type Index = DynamicIndex;
    type Init = AccessPointSummary;
    type Input = AccessPointRowMsg;
    type Output = AccessPointRowOutput;
    type ParentWidget = gtk::Box;
    type Root = gtk::Box;
    type Widgets = AccessPointRowWidgets;

    fn init_model(
        summary: Self::Init,
        _index: &DynamicIndex,
        _sender: FactorySender<Self>,
    ) -> Self {
        Self { summary }
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
            _main_box: main_box,
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
            AccessPointRowMsg::UpdateSummary(summary) => {
                self.summary = summary;
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

        let status = if self.summary.is_active {
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
