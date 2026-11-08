use std::fmt;

use zbus::zvariant::OwnedValue;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum State {
    #[default]
    /// Networking state is unknown. This indicates a daemon error that makes it
    /// unable to reasonably assess the state. In such event the applications
    /// are expected to assume Internet connectivity might be present and not
    /// disable controls that require network access. The graphical shells may
    /// hide the network accessibility indicator altogether since no meaningful
    /// status indication can be provided.
    Unknown = 0,

    /// Networking is not enabled, the system is being suspended or resumed from
    /// suspend.
    Asleep = 10,

    /// There is no active network connection. The graphical shell should
    /// indicate no network connectivity and the applications should not attempt
    /// to access the network.
    Disconnected = 20,

    /// Network connections are being cleaned up. The applications should tear
    /// down their network sessions.
    Disconnecting = 30,

    /// A network connection is being started The graphical shell should
    /// indicate the network is being connected while the applications should
    /// still make no attempts to connect the network.
    Connecting = 40,

    /// There is only local IPv4 and/or IPv6 connectivity, but no default route
    /// to access the Internet. The graphical shell should indicate no network
    /// connectivity.
    ConnectedLocal = 50,

    /// There is only site-wide IPv4 and/or IPv6 connectivity. This means a
    /// default route is available, but the Internet connectivity check (see
    /// "Connectivity" property) did not succeed. The graphical shell should
    /// indicate limited network connectivity.
    ConnectedSite = 60,

    /// There is global IPv4 and/or IPv6 Internet connectivity This means the
    /// Internet connectivity check succeeded, the graphical shell should
    /// indicate full network connectivity.
    ConnectedGlobal = 70,
}

impl From<u32> for State {
    fn from(value: u32) -> Self {
        match value {
            10 => Self::Asleep,
            20 => Self::Disconnected,
            30 => Self::Disconnecting,
            40 => Self::Connecting,
            50 => Self::ConnectedLocal,
            60 => Self::ConnectedSite,
            70 => Self::ConnectedGlobal,
            _ => Self::Unknown,
        }
    }
}

impl From<OwnedValue> for State {
    fn from(value: OwnedValue) -> Self {
        value
            .downcast_ref::<u32>()
            .map(Self::from)
            .unwrap_or_default()
    }
}

impl fmt::Display for State {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", match self {
            Self::Asleep => "Asleep",
            Self::ConnectedGlobal => "Global access",
            Self::ConnectedLocal => "Local access only",
            Self::ConnectedSite => "Site access only",
            Self::Connecting => "Connecting",
            Self::Disconnected => "Disconnected",
            Self::Disconnecting => "Disconnecting",
            Self::Unknown => "State unknown",
        })
    }
}

/// `NMDeviceType` value for ethernet devices.
pub const NM_DEVICE_TYPE_ETHERNET: u32 = 1;
/// `NMDeviceType` value for wifi devices.
pub const NM_DEVICE_TYPE_WIFI: u32 = 2;

/// `NMDeviceState` value for a fully activated device.
pub const NM_DEVICE_STATE_ACTIVATED: u32 = 100;
/// `NMDeviceState` value for a device that failed to connect.
pub const NM_DEVICE_STATE_FAILED: u32 = 120;

/// `NMDeviceStateReason` value for missing or rejected secrets, which is how
/// a wrong wifi password surfaces.
pub const NM_DEVICE_STATE_REASON_NO_SECRETS: u32 = 7;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ConnectivityState {
    #[default]

    /// Network connectivity is unknown. This means the connectivity checks are
    /// disabled (e.g. on server installations) or has not run yet. The
    /// graphical shell should assume the Internet connection might be available
    /// and not present a captive portal window.
    Unknown = 0,

    /// The host is not connected to any network. There's no active connection
    /// that contains a default route to the internet and thus it makes no sense
    /// to even attempt a connectivity check. The graphical shell should use
    /// this state to indicate the network connection is unavailable.
    None = 1,

    /// The Internet connection is hijacked by a captive portal gateway. The
    /// graphical shell may open a sandboxed web browser window (because the
    /// captive portals typically attempt a man-in-the-middle attacks against
    /// the https connections) for the purpose of authenticating to a gateway
    /// and retrigger the connectivity check with CheckConnectivity() when the
    /// browser window is dismissed.
    Portal = 2,

    /// The host is connected to a network, does not appear to be able to reach
    /// the full Internet, but a captive portal has not been detected.
    Limited = 3,

    /// The host is connected to a network, and appears to be able to reach the
    /// full Internet.
    Full = 4,
}

impl From<u32> for ConnectivityState {
    fn from(value: u32) -> Self {
        match value {
            1 => Self::None,
            2 => Self::Portal,
            3 => Self::Limited,
            4 => Self::Full,
            _ => Self::Unknown,
        }
    }
}

impl From<OwnedValue> for ConnectivityState {
    fn from(value: OwnedValue) -> Self {
        value
            .downcast_ref::<u32>()
            .map(Self::from)
            .unwrap_or_default()
    }
}

impl fmt::Display for ConnectivityState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", match self {
            ConnectivityState::Full => "Full connectivity",
            ConnectivityState::Limited => "Limited connectivity",
            ConnectivityState::None => "No connectivity",
            ConnectivityState::Portal => "Sign-in needed",
            ConnectivityState::Unknown => "Connectivity unknown",
        })
    }
}

/// Raw `NM80211ApFlags` bits from an access point's `Flags` property.
pub mod ap_flags {
    /// Access point requires authentication and encryption (usually WEP).
    pub const PRIVACY: u32 = 0x1;
}

/// Raw `NM80211ApSecurityFlags` bits from an access point's `WpaFlags` and
/// `RsnFlags` properties.
pub mod ap_security_flags {
    pub const KEY_MGMT_PSK: u32 = 0x100;
    pub const KEY_MGMT_802_1X: u32 = 0x200;
    pub const KEY_MGMT_SAE: u32 = 0x400;
    pub const KEY_MGMT_EAP_SUITE_B_192: u32 = 0x2000;
}

/// The security scheme an access point requires, derived from its `Flags`,
/// `WpaFlags`, and `RsnFlags` properties.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ApSecurity {
    /// No authentication required.
    #[default]
    Open,
    /// Legacy WEP (`Flags` has `PRIVACY` but no WPA/RSN key management).
    Wep,
    /// WPA/WPA2 pre-shared key. Password-only; supported by the inline
    /// connect flow.
    Psk,
    /// WPA3 Simultaneous Authentication of Equals. Password-only; supported
    /// by the inline connect flow.
    Sae,
    /// 802.1X or WPA3-Enterprise. Requires more than a single password (a
    /// username, certificate, or RADIUS-backed exchange), which the inline
    /// connect flow does not support.
    Enterprise,
}

impl ApSecurity {
    /// Derives the security scheme from an access point's raw `Flags`,
    /// `WpaFlags`, and `RsnFlags` bitfields.
    pub fn from_flags(flags: u32, wpa_flags: u32, rsn_flags: u32) -> Self {
        use ap_security_flags::{
            KEY_MGMT_802_1X, KEY_MGMT_EAP_SUITE_B_192, KEY_MGMT_PSK, KEY_MGMT_SAE,
        };

        let combined = wpa_flags | rsn_flags;

        if combined & KEY_MGMT_SAE != 0 {
            Self::Sae
        } else if combined & (KEY_MGMT_802_1X | KEY_MGMT_EAP_SUITE_B_192) != 0 {
            Self::Enterprise
        } else if combined & KEY_MGMT_PSK != 0 {
            Self::Psk
        } else if flags & ap_flags::PRIVACY != 0 {
            Self::Wep
        } else {
            Self::Open
        }
    }

    /// Whether this security scheme is unsecured.
    pub fn is_open(self) -> bool {
        self == Self::Open
    }

    /// Whether connecting requires a single password, which the inline
    /// connect flow (no secret agent) can satisfy.
    pub fn requires_password(self) -> bool {
        matches!(self, Self::Wep | Self::Psk | Self::Sae)
    }
}

impl fmt::Display for ApSecurity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", match self {
            Self::Open => "Open",
            Self::Wep => "WEP",
            Self::Psk => "WPA/WPA2 Personal",
            Self::Sae => "WPA3 Personal",
            Self::Enterprise => "Enterprise",
        })
    }
}

#[cfg(test)]
mod ap_security_tests {
    use super::*;

    #[test]
    fn open_network_has_no_flags() {
        assert_eq!(ApSecurity::from_flags(0, 0, 0), ApSecurity::Open);
    }

    #[test]
    fn wep_network_has_privacy_flag_only() {
        assert_eq!(
            ApSecurity::from_flags(ap_flags::PRIVACY, 0, 0),
            ApSecurity::Wep
        );
    }

    #[test]
    fn wpa2_psk_is_detected_from_rsn_flags() {
        assert_eq!(
            ApSecurity::from_flags(ap_flags::PRIVACY, 0, ap_security_flags::KEY_MGMT_PSK),
            ApSecurity::Psk
        );
    }

    #[test]
    fn wpa1_psk_is_detected_from_wpa_flags() {
        assert_eq!(
            ApSecurity::from_flags(ap_flags::PRIVACY, ap_security_flags::KEY_MGMT_PSK, 0),
            ApSecurity::Psk
        );
    }

    #[test]
    fn wpa3_sae_takes_priority_over_psk_in_transition_mode() {
        assert_eq!(
            ApSecurity::from_flags(
                ap_flags::PRIVACY,
                ap_security_flags::KEY_MGMT_PSK,
                ap_security_flags::KEY_MGMT_SAE
            ),
            ApSecurity::Sae
        );
    }

    #[test]
    fn enterprise_8021x_is_detected() {
        assert_eq!(
            ApSecurity::from_flags(ap_flags::PRIVACY, 0, ap_security_flags::KEY_MGMT_802_1X),
            ApSecurity::Enterprise
        );
    }

    #[test]
    fn enterprise_suite_b_192_is_detected() {
        assert_eq!(
            ApSecurity::from_flags(
                ap_flags::PRIVACY,
                0,
                ap_security_flags::KEY_MGMT_EAP_SUITE_B_192
            ),
            ApSecurity::Enterprise
        );
    }

    #[test]
    fn requires_password_excludes_open_and_enterprise() {
        assert!(!ApSecurity::Open.requires_password());
        assert!(ApSecurity::Wep.requires_password());
        assert!(ApSecurity::Psk.requires_password());
        assert!(ApSecurity::Sae.requires_password());
        assert!(!ApSecurity::Enterprise.requires_password());
    }
}
