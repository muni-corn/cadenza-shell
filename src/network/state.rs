use relm4::SharedState;

use crate::{
    network::types::{ConnectivityState, State},
    utils::icons::{
        NETWORK_WIFI_DISABLED, NETWORK_WIFI_ICON_NAMES, NETWORK_WIRED_CONNECTED,
        NETWORK_WIRED_DISABLED, percentage_to_icon_from_list,
    },
};

pub static NETWORK_STATE: SharedState<NetworkInfo> = SharedState::new();

#[derive(Debug, Clone)]
pub struct NetworkInfo {
    pub connection_state: State,
    pub connectivity: ConnectivityState,
    pub specific_info: Option<SpecificNetworkInfo>,
    /// Whether NetworkManager's wifi radio is enabled (the
    /// `WirelessEnabled` property). This is the correct source of truth for
    /// a wifi on/off toggle; `connection_state` reflects the overall
    /// connection, not the radio, and can be `Asleep` for reasons other than
    /// wifi being switched off.
    pub wifi_enabled: bool,
}

impl Default for NetworkInfo {
    fn default() -> Self {
        Self {
            connection_state: State::Unknown,
            connectivity: ConnectivityState::Unknown,
            specific_info: None,
            wifi_enabled: true,
        }
    }
}

impl NetworkInfo {
    pub fn wifi_ssid(&self) -> Option<String> {
        if let Some(SpecificNetworkInfo::WiFi { ref wifi_ssid, .. }) = self.specific_info {
            Some(wifi_ssid.clone())
        } else {
            None
        }
    }
}

#[derive(Clone, Debug)]
pub enum SpecificNetworkInfo {
    WiFi {
        wifi_ssid: String,
        wifi_strength: u8,
    },
    Wired,
}

/// Returns an appropriate icon name for the current networking state.
pub fn get_icon(info: &NetworkInfo) -> &str {
    if let State::Disconnected | State::Disconnecting | State::Asleep | State::Unknown =
        info.connection_state
    {
        return NETWORK_WIRED_DISABLED;
    }

    match info.specific_info {
        Some(SpecificNetworkInfo::WiFi { wifi_strength, .. }) => get_strength_icon(wifi_strength),
        Some(_) => NETWORK_WIRED_CONNECTED,
        None => NETWORK_WIFI_DISABLED,
    }
}

pub fn get_strength_icon(strength: u8) -> &'static str {
    percentage_to_icon_from_list(strength as f64 / 100.0, NETWORK_WIFI_ICON_NAMES)
}
