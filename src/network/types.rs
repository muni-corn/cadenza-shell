#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum State {
    #[default]
    Unknown = 0,
    Asleep = 10,
    Disconnected = 20,
    Disconnecting = 30,
    Connecting = 40,
    ConnectedLocal = 50,
    ConnectedSite = 60,
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

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum DeviceType {
    #[default]
    Unknown = 0,
    Generic = 14,
    Ethernet = 1,
    Wifi = 2,
    Bt = 5,
    OlpcMesh = 6,
    Wimax = 7,
    Modem = 8,
    Infiniband = 9,
    Bond = 10,
    Vlan = 11,
    Adsl = 12,
    Bridge = 13,
    Team = 15,
    Tun = 16,
    IpTunnel = 17,
    Macvlan = 18,
    Vxlan = 19,
    Veth = 20,
    Macsec = 21,
    Dummy = 22,
    Ppp = 23,
    OvsInterface = 24,
    OvsPort = 25,
    OvsBridge = 26,
    Wpan = 27,
    Lowpan6 = 28,
    Wireguard = 29,
    WifiP2p = 30,
    Vrf = 31,
    Loopback = 32,
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

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum DeviceState {
    #[default]
    Unknown = 0,
    Unmanaged = 10,
    Unavailable = 20,
    Disconnected = 30,
    Prepare = 40,
    Config = 50,
    NeedAuth = 60,
    IpConfig = 70,
    IpCheck = 80,
    Secondaries = 90,
    Activated = 100,
    Deactivating = 110,
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

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Connectivity {
    #[default]
    Unknown = 0,
    None = 1,
    Portal = 2,
    Limited = 3,
    Full = 4,
}

impl From<u32> for Connectivity {
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

#[derive(Debug, Clone)]
pub struct NetworkState {
    pub connected: bool,
    pub state: State,
    pub connectivity: Connectivity,
    pub device_type: DeviceType,
    pub wifi_ssid: String,
    pub wifi_strength: u8,
}
