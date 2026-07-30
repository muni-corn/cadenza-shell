use relm4::SharedState;
use zbus::zvariant::OwnedObjectPath;

use crate::{
    network::types::{ApSecurity, ConnectivityState, DeviceType, State},
    utils::icons::{
        NETWORK_WIFI_CONNECTING, NETWORK_WIFI_DISABLED, NETWORK_WIFI_DISCONNECTED,
        NETWORK_WIFI_ICON_NAMES, NETWORK_WIFI_NO_ROUTE, NETWORK_WIRED_CONNECTED,
        NETWORK_WIRED_CONNECTING, NETWORK_WIRED_NO_ROUTE, percentage_to_icon_from_list,
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

/// The kind of device backing the active or in-progress connection.
///
/// Unlike [`SpecificNetworkInfo`], which only populates once a connection is
/// fully activated, this is known as soon as NetworkManager picks a device
/// to activate - which is what lets [`get_icon`] show a wifi- or
/// ethernet-specific "acquiring" icon while still connecting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceKind {
    Wired,
    WiFi,
}

impl DeviceKind {
    /// Maps a NetworkManager device type to a `DeviceKind`, or `None` for
    /// device types this shell doesn't render distinctly (e.g. mobile
    /// broadband).
    pub fn from_device_type(device_type: DeviceType) -> Option<Self> {
        match device_type {
            DeviceType::Ethernet => Some(Self::Wired),
            DeviceType::Wifi => Some(Self::WiFi),
            _ => None,
        }
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
    /// The device kind backing the active or in-progress connection. See
    /// [`DeviceKind`] for how this differs from `specific_info`.
    pub device_kind: Option<DeviceKind>,
}

impl Default for NetworkInfo {
    fn default() -> Self {
        Self {
            connection_state: State::Unknown,
            connectivity: ConnectivityState::Unknown,
            specific_info: None,
            wifi_enabled: true,
            device_kind: None,
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

    /// Whether the connectivity check indicates a full route to the
    /// internet.
    ///
    /// `Unknown` counts as fine: the connectivity check is disabled on
    /// plenty of systems (e.g. `connectivity.uri` unset in NetworkManager's
    /// config), and treating that as a problem would render a permanent
    /// false warning.
    pub fn has_full_route(&self) -> bool {
        matches!(
            self.connectivity,
            ConnectivityState::Full | ConnectivityState::Unknown
        )
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
pub fn get_icon(info: &NetworkInfo) -> &'static str {
    if info.connection_state == State::Connecting {
        return match info.device_kind {
            Some(DeviceKind::Wired) => NETWORK_WIRED_CONNECTING,
            Some(DeviceKind::WiFi) | None => NETWORK_WIFI_CONNECTING,
        };
    }

    if let State::Disconnected | State::Disconnecting | State::Asleep | State::Unknown =
        info.connection_state
    {
        return if info.wifi_enabled {
            NETWORK_WIFI_DISCONNECTED
        } else {
            NETWORK_WIFI_DISABLED
        };
    }

    match info.specific_info {
        Some(SpecificNetworkInfo::WiFi { wifi_strength, .. }) => {
            if info.has_full_route() {
                get_strength_icon(wifi_strength)
            } else {
                NETWORK_WIFI_NO_ROUTE
            }
        }
        Some(SpecificNetworkInfo::Wired) => {
            if info.has_full_route() {
                NETWORK_WIRED_CONNECTED
            } else {
                NETWORK_WIRED_NO_ROUTE
            }
        }
        // connected, but to a device type we don't distinguish (e.g. mobile
        // broadband); there's no generic "connected" icon bundled, so this
        // falls back to the same icon as the wifi radio being off
        None => NETWORK_WIFI_DISABLED,
    }
}

pub fn get_strength_icon(strength: u8) -> &'static str {
    percentage_to_icon_from_list(strength as f64 / 100.0, NETWORK_WIFI_ICON_NAMES)
}

#[cfg(test)]
mod strength_icon_tests {
    use super::*;
    use crate::icon_names::{RADIOWAVES_1, RADIOWAVES_4};

    // radiowaves-1 is full signal and radiowaves-4 is empty; this pins that
    // relationship so the ordering doesn't get flipped again
    #[test]
    fn full_strength_uses_the_strongest_icon() {
        assert_eq!(get_strength_icon(100), RADIOWAVES_1);
    }

    #[test]
    fn no_strength_uses_the_weakest_icon() {
        assert_eq!(get_strength_icon(0), RADIOWAVES_4);
    }
}

#[cfg(test)]
mod get_icon_tests {
    use super::*;
    use crate::utils::icons::{NETWORK_WIFI_DISABLED, NETWORK_WIFI_DISCONNECTED};

    fn wifi_info(connectivity: ConnectivityState, strength: u8) -> NetworkInfo {
        NetworkInfo {
            connection_state: State::ConnectedGlobal,
            connectivity,
            specific_info: Some(SpecificNetworkInfo::WiFi {
                wifi_ssid: "test".to_string(),
                wifi_strength: strength,
            }),
            wifi_enabled: true,
            device_kind: Some(DeviceKind::WiFi),
        }
    }

    fn wired_info(connectivity: ConnectivityState) -> NetworkInfo {
        NetworkInfo {
            connection_state: State::ConnectedGlobal,
            connectivity,
            specific_info: Some(SpecificNetworkInfo::Wired),
            wifi_enabled: true,
            device_kind: Some(DeviceKind::Wired),
        }
    }

    #[test]
    fn disconnected_with_radio_on_shows_the_offline_icon() {
        let info = NetworkInfo {
            connection_state: State::Disconnected,
            wifi_enabled: true,
            ..Default::default()
        };
        assert_eq!(get_icon(&info), NETWORK_WIFI_DISCONNECTED);
    }

    #[test]
    fn disconnected_with_radio_off_shows_the_disabled_icon() {
        let info = NetworkInfo {
            connection_state: State::Disconnected,
            wifi_enabled: false,
            ..Default::default()
        };
        assert_eq!(get_icon(&info), NETWORK_WIFI_DISABLED);
    }

    #[test]
    fn wifi_with_full_route_shows_the_strength_icon() {
        assert_eq!(
            get_icon(&wifi_info(ConnectivityState::Full, 100)),
            get_strength_icon(100)
        );
    }

    #[test]
    fn wifi_with_unknown_connectivity_still_shows_the_strength_icon() {
        assert_eq!(
            get_icon(&wifi_info(ConnectivityState::Unknown, 100)),
            get_strength_icon(100)
        );
    }

    #[test]
    fn wifi_without_a_full_route_shows_the_no_route_icon() {
        for connectivity in [
            ConnectivityState::None,
            ConnectivityState::Portal,
            ConnectivityState::Limited,
        ] {
            assert_eq!(
                get_icon(&wifi_info(connectivity, 100)),
                NETWORK_WIFI_NO_ROUTE
            );
        }
    }

    #[test]
    fn wired_with_full_route_shows_the_connected_icon() {
        assert_eq!(
            get_icon(&wired_info(ConnectivityState::Full)),
            NETWORK_WIRED_CONNECTED
        );
    }

    #[test]
    fn wired_without_a_full_route_shows_the_no_route_icon() {
        assert_eq!(
            get_icon(&wired_info(ConnectivityState::Limited)),
            NETWORK_WIRED_NO_ROUTE
        );
    }

    #[test]
    fn connecting_over_wifi_shows_the_wifi_acquiring_icon() {
        let info = NetworkInfo {
            connection_state: State::Connecting,
            device_kind: Some(DeviceKind::WiFi),
            ..Default::default()
        };
        assert_eq!(get_icon(&info), NETWORK_WIFI_CONNECTING);
    }

    #[test]
    fn connecting_over_wired_shows_the_wired_acquiring_icon() {
        let info = NetworkInfo {
            connection_state: State::Connecting,
            device_kind: Some(DeviceKind::Wired),
            ..Default::default()
        };
        assert_eq!(get_icon(&info), NETWORK_WIRED_CONNECTING);
    }

    #[test]
    fn connecting_with_unknown_device_kind_falls_back_to_wifi_acquiring_icon() {
        let info = NetworkInfo {
            connection_state: State::Connecting,
            device_kind: None,
            ..Default::default()
        };
        assert_eq!(get_icon(&info), NETWORK_WIFI_CONNECTING);
    }
}
