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

/// `NMActiveConnectionState` value for a fully activated connection.
pub const NM_ACTIVE_CONNECTION_STATE_ACTIVATED: u32 = 2;
/// `NMActiveConnectionState` value for a connection that has gone away,
/// including attempts that failed.
pub const NM_ACTIVE_CONNECTION_STATE_DEACTIVATED: u32 = 4;

/// `NMActiveConnectionStateReason` value for missing or rejected secrets,
/// which is how a wrong wifi password surfaces.
pub const NM_ACTIVE_CONNECTION_STATE_REASON_NO_SECRETS: u32 = 9;

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
