use relm4::SharedState;
use zbus::zvariant::OwnedObjectPath;

use crate::{
    network::types::{ApSecurity, ConnectivityState, State},
    utils::icons::{
        NETWORK_WIFI_DISABLED, NETWORK_WIFI_ICON_NAMES, NETWORK_WIRED_CONNECTED,
        NETWORK_WIRED_DISABLED, percentage_to_icon_from_list,
    },
};

pub static NETWORK_STATE: SharedState<NetworkInfo> = SharedState::new();

/// Wifi scan results and saved connection info for the network menu.
///
/// Kept separate from [`NETWORK_STATE`] so the bar tile - which only cares
/// about the active connection - doesn't get a fresh clone of the whole
/// access point list on every scan; only the menu subscribes to this.
pub static WIFI_SCAN_STATE: SharedState<WifiScanState> = SharedState::new();

#[derive(Debug, Clone, Default)]
pub struct WifiScanState {
    /// Access points, deduplicated by SSID and sorted with the active
    /// connection first, then by descending signal strength.
    pub access_points: Vec<AccessPointSummary>,
    /// Whether a scan (`RequestScan`) is currently in flight.
    pub scanning: bool,
    /// `WirelessDevice.LastScan`, in CLOCK_BOOTTIME milliseconds; `-1` if a
    /// scan has never completed. Used to avoid rescanning too frequently.
    pub last_scan_ms: i64,
}

/// A single wifi network as shown in the menu: one row per SSID, even if
/// multiple access points (BSSIDs) are broadcasting it.
#[derive(Debug, Clone, PartialEq)]
pub struct AccessPointSummary {
    pub ssid: String,
    /// Signal strength of the strongest access point broadcasting this SSID,
    /// as a percentage.
    pub strength: u8,
    pub security: ApSecurity,
    /// Whether this is the network the active connection is on.
    pub is_active: bool,
    /// The object path of a saved connection profile matching this SSID, if
    /// one exists. `Some` means connecting won't need a password (NM already
    /// has the secrets); it's also required to "forget" the network.
    pub saved_connection: Option<OwnedObjectPath>,
}

// is_saved/needs_password aren't called yet; wired up when network_menu
// renders the access point list
#[allow(dead_code)]
impl AccessPointSummary {
    pub fn is_saved(&self) -> bool {
        self.saved_connection.is_some()
    }

    /// Whether connecting to this network needs a password from the user
    /// (i.e. it's secured and NM doesn't already have saved secrets for it).
    pub fn needs_password(&self) -> bool {
        self.security.requires_password() && !self.is_saved()
    }
}

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
