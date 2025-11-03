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

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum DeviceType {
    #[default]
    /// Unknown device.
    Unknown = 0,

    /// Generic support for unrecognized device types.
    Generic = 14,

    /// A wired ethernet device.
    Ethernet = 1,

    /// An 802.11 Wi-Fi device.
    Wifi = 2,

    /// Not used.
    Unused1 = 3,

    /// Not used.
    Unused2 = 4,

    /// A Bluetooth device supporting PAN or DUN access protocols.
    Bt = 5,

    /// An OLPC XO mesh networking device.
    OlpcMesh = 6,

    /// An 802.16e Mobile WiMAX broadband device.
    Wimax = 7,

    /// A modem supporting analog telephone, CDMA/EVDO, GSM/UMTS, or LTE network
    /// access protocols.
    Modem = 8,

    /// An IP-over-InfiniBand device.
    Infiniband = 9,

    /// A bond controller interface.
    Bond = 10,

    /// An 802.1Q VLAN interface.
    Vlan = 11,

    /// ADSL modem.
    Adsl = 12,

    /// A bridge controller interface.
    Bridge = 13,

    /// A team controller interface.
    Team = 15,

    /// A TUN or TAP interface.
    Tun = 16,

    /// An IP tunnel interface.
    IpTunnel = 17,

    /// A MACVLAN interface.
    Macvlan = 18,

    /// A VXLAN interface.
    Vxlan = 19,

    /// A VETH interface.
    Veth = 20,

    /// A MACsec interface.
    Macsec = 21,

    /// A dummy interface.
    Dummy = 22,

    /// A PPP interface.
    Ppp = 23,

    /// An Open vSwitch interface.
    OvsInterface = 24,

    /// An Open vSwitch port.
    OvsPort = 25,

    /// An Open vSwitch bridge.
    OvsBridge = 26,

    /// An IEEE 802.15.4 (WPAN) MAC Layer Device.
    Wpan = 27,

    /// 6LoWPAN interface.
    Lowpan6 = 28,

    /// A WireGuard interface.
    Wireguard = 29,

    /// An 802.11 Wi-Fi P2P device. Since: 1.16.
    WifiP2p = 30,

    /// A VRF (Virtual Routing and Forwarding) interface. Since: 1.24.
    Vrf = 31,

    /// A loopback interface. Since: 1.42.
    Loopback = 32,

    /// An HSR/PRP device. Since: 1.46.
    Hsr = 33,

    /// An IPVLAN device. Since: 1.52.
    Ipvlan = 34,
}

impl From<u32> for DeviceType {
    fn from(value: u32) -> Self {
        match value {
            1 => Self::Ethernet,
            2 => Self::Wifi,
            5 => Self::Bt,
            6 => Self::OlpcMesh,
            7 => Self::Wimax,
            8 => Self::Modem,
            9 => Self::Infiniband,
            10 => Self::Bond,
            11 => Self::Vlan,
            12 => Self::Adsl,
            13 => Self::Bridge,
            14 => Self::Generic,
            15 => Self::Team,
            16 => Self::Tun,
            17 => Self::IpTunnel,
            18 => Self::Macvlan,
            19 => Self::Vxlan,
            20 => Self::Veth,
            21 => Self::Macsec,
            22 => Self::Dummy,
            23 => Self::Ppp,
            24 => Self::OvsInterface,
            25 => Self::OvsPort,
            26 => Self::OvsBridge,
            27 => Self::Wpan,
            28 => Self::Lowpan6,
            29 => Self::Wireguard,
            30 => Self::WifiP2p,
            31 => Self::Vrf,
            32 => Self::Loopback,
            _ => Self::Unknown,
        }
    }
}

impl From<OwnedValue> for DeviceType {
    fn from(value: OwnedValue) -> Self {
        value
            .downcast_ref::<u32>()
            .map(Self::from)
            .unwrap_or_default()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum DeviceState {
    #[default]

    /// The device's state is unknown.
    Unknown = 0,

    /// The device is recognized, but not managed by NetworkManager.
    Unmanaged = 10,

    /// The device is managed by NetworkManager, but is not available for use.
    /// Reasons may include the wireless switched off, missing firmware, no
    /// ethernet carrier, missing supplicant or modem manager, etc.
    Unavailable = 20,

    /// The device can be activated, but is currently idle and not connected to
    /// a network.
    Disconnected = 30,

    /// The device is preparing the connection to the network. This may include
    /// operations like changing the MAC address, setting physical link
    /// properties, and anything else required to connect to the requested
    /// network.
    Prepare = 40,

    /// The device is connecting to the requested network. This may include
    /// operations like associating with the Wi-Fi AP, dialing the modem,
    /// connecting to the remote Bluetooth device, etc.
    Config = 50,

    /// The device requires more information to continue connecting to the
    /// requested network. This includes secrets like WiFi passphrases, login
    /// passwords, PIN codes, etc.
    NeedAuth = 60,

    /// The device is requesting IPv4 and/or IPv6 addresses and routing
    /// information from the network.
    IpConfig = 70,

    /// The device is checking whether further action is required for the
    /// requested network connection. This may include checking whether only
    /// local network access is available, whether a captive portal is blocking
    /// access to the Internet, etc.
    IpCheck = 80,

    /// The device is waiting for a secondary connection (like a VPN) which must
    /// activated before the device can be activated.
    Secondaries = 90,

    /// The device has a network connection, either local or global.
    Activated = 100,

    /// A disconnection from the current network connection was requested, and
    /// the device is cleaning up resources used for that connection. The
    /// network connection may still be valid.
    Deactivating = 110,

    /// The device failed to connect to the requested network and is cleaning up
    /// the connection request.
    Failed = 120,
}

impl From<u32> for DeviceState {
    fn from(value: u32) -> Self {
        match value {
            10 => Self::Unmanaged,
            20 => Self::Unavailable,
            30 => Self::Disconnected,
            40 => Self::Prepare,
            50 => Self::Config,
            60 => Self::NeedAuth,
            70 => Self::IpConfig,
            80 => Self::IpCheck,
            90 => Self::Secondaries,
            100 => Self::Activated,
            110 => Self::Deactivating,
            120 => Self::Failed,
            _ => Self::Unknown,
        }
    }
}

impl From<OwnedValue> for DeviceState {
    fn from(value: OwnedValue) -> Self {
        value
            .downcast_ref::<u32>()
            .map(Self::from)
            .unwrap_or_default()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum DeviceStateReason {
    #[default]

    /// No reason given
    None = 0,

    /// Unknown error
    Unknown = 1,

    /// Device is now managed
    NowManaged = 2,

    /// Device is now unmanaged
    NowUnmanaged = 3,

    /// The device could not be readied for configuration
    ConfigFailed = 4,

    /// IP configuration could not be reserved (no available address, timeout,
    /// etc)
    IpConfigUnavailable = 5,

    /// The IP config is no longer valid
    IpConfigExpired = 6,

    /// Secrets were required, but not provided
    NoSecrets = 7,

    /// 802.1x supplicant disconnected
    SupplicantDisconnect = 8,

    /// 802.1x supplicant configuration failed
    SupplicantConfigFailed = 9,

    /// 802.1x supplicant failed
    SupplicantFailed = 10,

    /// 802.1x supplicant took too long to authenticate
    SupplicantTimeout = 11,

    /// PPP service failed to start
    PppStartFailed = 12,

    /// PPP service disconnected
    PppDisconnect = 13,

    /// PPP failed
    PppFailed = 14,

    /// DHCP client failed to start
    DhcpStartFailed = 15,

    /// DHCP client error
    DhcpError = 16,

    /// DHCP client failed
    DhcpFailed = 17,

    /// Shared connection service failed to start
    SharedStartFailed = 18,

    /// Shared connection service failed
    SharedFailed = 19,

    /// AutoIP service failed to start
    AutoipStartFailed = 20,

    /// AutoIP service error
    AutoipError = 21,

    /// AutoIP service failed
    AutoipFailed = 22,

    /// The line is busy
    ModemBusy = 23,

    /// No dial tone
    ModemNoDialTone = 24,

    /// No carrier could be established
    ModemNoCarrier = 25,

    /// The dialing request timed out
    ModemDialTimeout = 26,

    /// The dialing attempt failed
    ModemDialFailed = 27,

    /// Modem initialization failed
    ModemInitFailed = 28,

    /// Failed to select the specified APN
    GsmApnFailed = 29,

    /// Not searching for networks
    GsmRegistrationNotSearching = 30,

    /// Network registration denied
    GsmRegistrationDenied = 31,

    /// Network registration timed out
    GsmRegistrationTimeout = 32,

    /// Failed to register with the requested network
    GsmRegistrationFailed = 33,

    /// PIN check failed
    GsmPinCheckFailed = 34,

    /// Necessary firmware for the device may be missing
    FirmwareMissing = 35,

    /// The device was removed
    Removed = 36,

    /// NetworkManager went to sleep
    Sleeping = 37,

    /// The device's active connection disappeared
    ConnectionRemoved = 38,

    /// Device disconnected by user or client
    UserRequested = 39,

    /// Carrier/link changed
    Carrier = 40,

    /// The device's existing connection was assumed
    ConnectionAssumed = 41,

    /// The supplicant is now available
    SupplicantAvailable = 42,

    /// The modem could not be found
    ModemNotFound = 43,

    /// The Bluetooth connection failed or timed out
    BtFailed = 44,

    /// GSM Modem's SIM Card not inserted
    GsmSimNotInserted = 45,

    /// GSM Modem's SIM Pin required
    GsmSimPinRequired = 46,

    /// GSM Modem's SIM Puk required
    GsmSimPukRequired = 47,

    /// GSM Modem's SIM wrong
    GsmSimWrong = 48,

    /// InfiniBand device does not support connected mode
    InfinibandMode = 49,

    /// A dependency of the connection failed
    DependencyFailed = 50,

    /// Problem with the RFC 2684 Ethernet over ADSL bridge
    Br2684Failed = 51,

    /// ModemManager not running
    ModemManagerUnavailable = 52,

    /// The Wi-Fi network could not be found
    SsidNotFound = 53,

    /// A secondary connection of the base connection failed
    SecondaryConnectionFailed = 54,

    /// DCB or FCoE setup failed
    DcbFcoeFailed = 55,

    /// teamd control failed
    TeamdControlFailed = 56,

    /// Modem failed or no longer available
    ModemFailed = 57,

    /// Modem now ready and available
    ModemAvailable = 58,

    /// SIM PIN was incorrect
    SimPinIncorrect = 59,

    /// New connection activation was enqueued
    NewActivation = 60,

    /// the device's parent changed
    ParentChanged = 61,

    /// the device parent's management changed
    ParentManagedChanged = 62,

    /// problem communicating with Open vSwitch database
    OvsdbFailed = 63,

    /// a duplicate IP address was detected
    IpAddressDuplicate = 64,

    /// The selected IP method is not supported
    IpMethodUnsupported = 65,

    /// configuration of SR-IOV parameters failed
    SriovConfigurationFailed = 66,

    /// The Wi-Fi P2P peer could not be found
    PeerNotFound = 67,

    /// The device handler dispatcher returned an error. Since: 1.46
    DeviceHandlerFailed = 68,

    /// The device is unmanaged because the device type is unmanaged by default.
    /// Since: 1.48
    UnmanagedByDefault = 69,

    /// The device is unmanaged because it is an external device and is
    /// unconfigured (down or without addresses). Since: 1.48
    UnmanagedExternalDown = 70,

    /// The device is unmanaged because the link is not initialized by udev.
    /// Since: 1.48
    UnmanagedLinkNotInit = 71,

    /// The device is unmanaged because NetworkManager is quitting. Since: 1.48
    UnmanagedQuitting = 72,

    /// The device is unmanaged because networking is disabled or the system is
    /// suspended. Since: 1.48
    UnmanagedSleeping = 73,

    /// The device is unmanaged by user decision in NetworkManager.conf
    /// ('unmanaged' in a [device*] section). Since: 1.48
    UnmanagedUserConf = 74,

    /// The device is unmanaged by explicit user decision (e.g. 'nmcli device
    /// set $DEV managed no'). Since: 1.48
    UnmanagedUserExplicit = 75,

    /// The device is unmanaged by user decision via settings plugin
    /// ('unmanaged-devices' for keyfile or 'NM_CONTROLLED=no' for ifcfg-rh).
    /// Since: 1.48
    UnmanagedUserSettings = 76,

    /// The device is unmanaged via udev rule. Since: 1.48
    UnmanagedUserUdev = 77,
}

impl From<u32> for DeviceStateReason {
    fn from(value: u32) -> Self {
        match value {
            0 => Self::None,
            2 => Self::NowManaged,
            3 => Self::NowUnmanaged,
            4 => Self::ConfigFailed,
            5 => Self::IpConfigUnavailable,
            6 => Self::IpConfigExpired,
            7 => Self::NoSecrets,
            8 => Self::SupplicantDisconnect,
            9 => Self::SupplicantConfigFailed,
            10 => Self::SupplicantFailed,
            11 => Self::SupplicantTimeout,
            12 => Self::PppStartFailed,
            13 => Self::PppDisconnect,
            14 => Self::PppFailed,
            15 => Self::DhcpStartFailed,
            16 => Self::DhcpError,
            17 => Self::DhcpFailed,
            18 => Self::SharedStartFailed,
            19 => Self::SharedFailed,
            20 => Self::AutoipStartFailed,
            21 => Self::AutoipError,
            22 => Self::AutoipFailed,
            23 => Self::ModemBusy,
            24 => Self::ModemNoDialTone,
            25 => Self::ModemNoCarrier,
            26 => Self::ModemDialTimeout,
            27 => Self::ModemDialFailed,
            28 => Self::ModemInitFailed,
            29 => Self::GsmApnFailed,
            30 => Self::GsmRegistrationNotSearching,
            31 => Self::GsmRegistrationDenied,
            32 => Self::GsmRegistrationTimeout,
            33 => Self::GsmRegistrationFailed,
            34 => Self::GsmPinCheckFailed,
            35 => Self::FirmwareMissing,
            36 => Self::Removed,
            37 => Self::Sleeping,
            38 => Self::ConnectionRemoved,
            39 => Self::UserRequested,
            40 => Self::Carrier,
            41 => Self::ConnectionAssumed,
            42 => Self::SupplicantAvailable,
            43 => Self::ModemNotFound,
            44 => Self::BtFailed,
            45 => Self::GsmSimNotInserted,
            46 => Self::GsmSimPinRequired,
            47 => Self::GsmSimPukRequired,
            48 => Self::GsmSimWrong,
            49 => Self::InfinibandMode,
            50 => Self::DependencyFailed,
            51 => Self::Br2684Failed,
            52 => Self::ModemManagerUnavailable,
            53 => Self::SsidNotFound,
            54 => Self::SecondaryConnectionFailed,
            55 => Self::DcbFcoeFailed,
            56 => Self::TeamdControlFailed,
            57 => Self::ModemFailed,
            58 => Self::ModemAvailable,
            59 => Self::SimPinIncorrect,
            60 => Self::NewActivation,
            61 => Self::ParentChanged,
            _ => Self::Unknown,
        }
    }
}

impl From<OwnedValue> for DeviceStateReason {
    fn from(value: OwnedValue) -> Self {
        value
            .downcast_ref::<u32>()
            .map(Self::from)
            .unwrap_or_default()
    }
}

pub enum ActiveConnectionState {
    /// The state of the connection is unknown.
    Unknown = 0,

    /// A network connection is being prepared.
    Activating = 1,

    /// There is a connection to the network.
    Activated = 2,

    /// The network connection is being torn down and cleaned up.
    Deactivating = 3,

    /// The network connection is disconnected and will be removed.
    Deactivated = 4,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ActiveConnectionStateReason {
    #[default]

    /// The reason for the active connection state change is unknown.
    Unknown = 0,

    /// No reason was given for the active connection state change.
    None = 1,

    /// The active connection changed state because the user disconnected it.
    UserDisconnected = 2,

    /// The active connection changed state because the device it was using was
    /// disconnected.
    DeviceDisconnected = 3,

    /// The service providing the VPN connection was stopped.
    ServiceStopped = 4,

    /// The IP config of the active connection was invalid.
    IpConfigInvalid = 5,

    /// The connection attempt to the VPN service timed out.
    ConnectTimeout = 6,

    /// A timeout occurred while starting the service providing the VPN
    /// connection.
    ServiceStartTimeout = 7,

    /// Starting the service providing the VPN connection failed.
    ServiceStartFailed = 8,

    /// Necessary secrets for the connection were not provided.
    NoSecrets = 9,

    /// Authentication to the server failed.
    LoginFailed = 10,

    /// The connection was deleted from settings.
    ConnectionRemoved = 11,

    /// Master connection of this connection failed to activate.
    DependencyFailed = 12,

    /// Could not create the software device link.
    DeviceRealizeFailed = 13,

    /// The device this connection depended on disappeared.
    DeviceRemoved = 14,
}

impl From<u32> for ActiveConnectionStateReason {
    fn from(value: u32) -> Self {
        match value {
            1 => Self::None,
            2 => Self::UserDisconnected,
            3 => Self::DeviceDisconnected,
            4 => Self::ServiceStopped,
            5 => Self::IpConfigInvalid,
            6 => Self::ConnectTimeout,
            7 => Self::ServiceStartTimeout,
            8 => Self::ServiceStartFailed,
            9 => Self::NoSecrets,
            10 => Self::LoginFailed,
            11 => Self::ConnectionRemoved,
            12 => Self::DependencyFailed,
            13 => Self::DeviceRealizeFailed,
            14 => Self::DeviceRemoved,
            _ => Self::Unknown,
        }
    }
}

impl From<OwnedValue> for ActiveConnectionStateReason {
    fn from(value: OwnedValue) -> Self {
        value
            .downcast_ref::<u32>()
            .map(Self::from)
            .unwrap_or_default()
    }
}

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
