//! How the network menu reacts to messages and connection outcomes.

use std::time::Duration;

use relm4::ComponentSender;

use super::{
    NetworkMenu, NetworkMenuCommand, NetworkMenuMsg, PasswordPrompt, rows::update_ap_rows,
};
use crate::{
    network::{self, AccessPointSummary, ApSecurity, ConnectFailureReason, NetworkEvent},
    settings,
};

impl NetworkMenu {
    /// The latest scan result for a network.
    fn access_point(&self, ssid: &str) -> Option<AccessPointSummary> {
        self.scan_state
            .access_points
            .iter()
            .find(|ap| ap.ssid == ssid)
            .cloned()
    }

    /// Applies one menu message to the model.
    pub(super) fn update_message(&mut self, msg: NetworkMenuMsg, sender: &ComponentSender<Self>) {
        match msg {
            NetworkMenuMsg::UpdateState(state) => {
                self.network_state = state;
                self.resync_wifi_switch = true;
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
                    self.schedule_rescan(sender);
                }
            }
            NetworkMenuMsg::Disconnect => {
                network::disconnect();
            }
            NetworkMenuMsg::ApClicked(ssid) => {
                let Some(summary) = self.access_point(&ssid) else {
                    return;
                };
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
                        saved_connections: Vec::new(),
                    });
                } else {
                    network::connect(
                        summary.ssid,
                        summary.security,
                        None,
                        summary.saved_connections,
                    );
                }
            }
            NetworkMenuMsg::ForgetClicked(ssid) => {
                if let Some(summary) = self.access_point(&ssid)
                    && summary.is_saved()
                {
                    network::forget(summary.saved_connections);
                }
            }
            NetworkMenuMsg::PasswordSubmitted(password) => {
                // keep the prompt open so the password can be corrected
                if let Some(prompt) = &self.password_prompt
                    && let Err(problem) = prompt.security.validate_password(password.expose())
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
                        prompt.saved_connections,
                    );
                }
            }
            NetworkMenuMsg::CancelPassword => {
                self.password_prompt = None;
                self.connect_error = None;
            }
        }
    }

    /// Requests another scan after `network.scan_interval` while the menu
    /// stays open. RequestScan only starts a scan, so repeating it is what
    /// eventually surfaces fresh results.
    pub(super) fn schedule_rescan(&self, sender: &ComponentSender<Self>) {
        let generation = self.rescan_generation;
        let interval = Duration::from_secs(settings::get_config().network.scan_interval);
        sender.oneshot_command(async move {
            tokio::time::sleep(interval).await;
            NetworkMenuCommand::RescanTick(generation)
        });
    }

    pub(super) fn handle_connection_event(&mut self, event: NetworkEvent) {
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
                        .map(|ap| (ap.security, ap.saved_connections.clone()));
                    if let Some((security, saved_connections)) = network {
                        self.password_prompt = Some(PasswordPrompt {
                            ssid,
                            security,
                            saved_connections,
                        });
                    }
                }
            }
        }
    }
}
