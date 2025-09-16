use futures_lite::StreamExt;
use relm4::{Worker, prelude::*};
use zbus::proxy;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum NetworkState {
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

impl From<u32> for NetworkState {
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
pub enum ConnectivityState {
    #[default]
    Unknown = 0,
    None = 1,
    Portal = 2,
    Limited = 3,
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

#[derive(Debug, Clone)]
pub struct NetworkInfo {
    pub connected: bool,
    pub state: NetworkState,
    pub connectivity: ConnectivityState,
    pub device_type: DeviceType,
    pub wifi_ssid: String,
    pub wifi_strength: u8,
}

#[derive(Debug)]
pub enum NetworkWorkerMsg {
    Update,
}

#[derive(Debug)]
pub enum NetworkWorkerOutput {
    StateChanged(NetworkInfo),
}

#[proxy(
    interface = "org.freedesktop.NetworkManager",
    default_service = "org.freedesktop.NetworkManager",
    default_path = "/org/freedesktop/NetworkManager"
)]
trait NetworkManager {
    #[zbus(property)]
    fn state(&self) -> zbus::Result<u32>;

    #[zbus(property)]
    fn connectivity(&self) -> zbus::Result<u32>;

    #[zbus(property)]
    fn primary_connection(&self) -> zbus::Result<zbus::zvariant::OwnedObjectPath>;

    fn get_devices(&self) -> zbus::Result<Vec<zbus::zvariant::OwnedObjectPath>>;
}

#[proxy(
    interface = "org.freedesktop.NetworkManager.Device",
    default_service = "org.freedesktop.NetworkManager"
)]
trait NetworkDevice {
    #[zbus(property)]
    fn device_type(&self) -> zbus::Result<u32>;

    #[zbus(property)]
    fn interface(&self) -> zbus::Result<String>;

    #[zbus(property)]
    fn state(&self) -> zbus::Result<u32>;
}

#[proxy(
    interface = "org.freedesktop.NetworkManager.Device.Wireless",
    default_service = "org.freedesktop.NetworkManager"
)]
trait WirelessDevice {
    #[zbus(property)]
    fn active_access_point(&self) -> zbus::Result<zbus::zvariant::OwnedObjectPath>;
}

#[proxy(
    interface = "org.freedesktop.NetworkManager.AccessPoint",
    default_service = "org.freedesktop.NetworkManager"
)]
trait AccessPoint {
    #[zbus(property)]
    fn ssid(&self) -> zbus::Result<Vec<u8>>;

    #[zbus(property)]
    fn strength(&self) -> zbus::Result<u8>;
}

#[derive(Debug)]
pub struct NetworkService;

impl Default for NetworkInfo {
    fn default() -> Self {
        Self {
            connected: false,
            state: NetworkState::Unknown,
            connectivity: ConnectivityState::Unknown,
            device_type: DeviceType::Unknown,
            wifi_ssid: String::new(),
            wifi_strength: 0,
        }
    }
}

impl Worker for NetworkService {
    type Init = ();
    type Input = NetworkWorkerMsg;
    type Output = NetworkWorkerOutput;

    fn init(_init: Self::Init, sender: ComponentSender<Self>) -> Self {
        // setup property watching
        let sender_clone = sender.clone();
        relm4::spawn(async move {
            if let Err(e) = Self::setup_property_watching(sender_clone).await {
                log::error!("failed to setup network property watching: {}", e);
            }
        });

        // initial fetch
        let sender_init = sender.clone();
        relm4::spawn(async move {
            let info = Self::fetch_network_info().await.unwrap_or_default();
            sender_init
                .output(NetworkWorkerOutput::StateChanged(info))
                .unwrap_or_else(|e| log::error!("failed to send initial network state: {:?}", e));
        });

        Self
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            NetworkWorkerMsg::Update => {
                let sender_clone = sender.clone();
                relm4::spawn(async move {
                    let info = Self::fetch_network_info().await.unwrap_or_default();
                    sender_clone
                        .output(NetworkWorkerOutput::StateChanged(info))
                        .unwrap_or_else(|e| log::error!("failed to send network state: {:?}", e));
                });
            }
        }
    }
}

impl NetworkService {
    async fn setup_property_watching(sender: ComponentSender<Self>) -> anyhow::Result<()> {
        let conn = zbus::Connection::system().await?;
        let nm_proxy = NetworkManagerProxy::new(&conn).await?;

        // watch for state changes
        let mut state_stream = nm_proxy.receive_state_changed().await;
        let sender_state = sender.clone();
        relm4::spawn(async move {
            while let Some(_change) = state_stream.next().await {
                sender_state.input(NetworkWorkerMsg::Update);
            }
        });

        // watch for connectivity changes
        let mut connectivity_stream = nm_proxy.receive_connectivity_changed().await;
        let sender_connectivity = sender.clone();
        relm4::spawn(async move {
            while let Some(_change) = connectivity_stream.next().await {
                sender_connectivity.input(NetworkWorkerMsg::Update);
            }
        });

        // watch for primary connection changes
        let mut primary_connection_stream = nm_proxy.receive_primary_connection_changed().await;
        let sender_primary = sender.clone();
        relm4::spawn(async move {
            while let Some(_change) = primary_connection_stream.next().await {
                sender_primary.input(NetworkWorkerMsg::Update);
            }
        });

        Ok(())
    }

    async fn fetch_network_info() -> anyhow::Result<NetworkInfo> {
        let conn = zbus::Connection::system().await?;
        let nm_proxy = NetworkManagerProxy::new(&conn).await?;

        // get overall state
        let state = NetworkState::from(nm_proxy.state().await?);
        let is_connected = matches!(
            state,
            NetworkState::ConnectedLocal
                | NetworkState::ConnectedSite
                | NetworkState::ConnectedGlobal
        );

        // get connectivity
        let connectivity = ConnectivityState::from(nm_proxy.connectivity().await?);

        let mut info = NetworkInfo {
            connected: is_connected,
            state,
            connectivity,
            device_type: DeviceType::Unknown,
            wifi_ssid: String::new(),
            wifi_strength: 0,
        };

        if is_connected {
            match nm_proxy.get_devices().await {
                Ok(devices) => {
                    // find the active device
                    for device_path in devices {
                        let device_proxy = NetworkDeviceProxy::builder(&conn)
                            .path(&device_path)?
                            .build()
                            .await?;

                        let device_type = DeviceType::from(device_proxy.device_type().await?);
                        let device_state = DeviceState::from(device_proxy.state().await?);

                        // only activated devices are considered active for primary connection
                        if device_state == DeviceState::Activated {
                            info.device_type = device_type;

                            if device_type == DeviceType::Wifi {
                                // get wifi specific info
                                match Self::get_wifi_info(&conn, &device_path).await {
                                    Ok(wifi_info) => {
                                        info.wifi_ssid = wifi_info.0;
                                        info.wifi_strength = wifi_info.1;
                                        // WiFi info successfully retrieved
                                    }
                                    Err(e) => {
                                        log::error!("failed to get WiFi info: {}", e);
                                        // try to get at least the interface name as fallback
                                        if let Ok(interface) = device_proxy.interface().await {
                                            info.wifi_ssid = interface;
                                        }
                                    }
                                }
                                break; // Found our primary WiFi connection
                            }
                            // For non-WiFi activated devices, set as primary
                            // but continue looking for WiFi
                        } else if device_type == DeviceType::Wifi {
                            // Even if WiFi is not activated, try to get the SSID if connecting
                            if matches!(
                                device_state,
                                DeviceState::Config | DeviceState::IpConfig | DeviceState::IpCheck
                            ) {
                                match Self::get_wifi_info(&conn, &device_path).await {
                                    Ok(wifi_info) => {
                                        info.device_type = device_type;
                                        info.wifi_ssid = wifi_info.0;
                                        info.wifi_strength = wifi_info.1;
                                        log::debug!(
                                            "wifi connecting/configuring: ssid='{}', strength={}",
                                            info.wifi_ssid,
                                            info.wifi_strength
                                        );
                                        break;
                                    }
                                    Err(e) => {
                                        log::debug!(
                                            "wifi device in progress but no ssid available: {}",
                                            e
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    log::error!("failed to get devices: {}", e);
                }
            }
        }

        Ok(info)
    }

    async fn get_wifi_info(
        conn: &zbus::Connection,
        device_path: &zbus::zvariant::OwnedObjectPath,
    ) -> anyhow::Result<(String, u8)> {
        let wifi_proxy = WirelessDeviceProxy::builder(conn)
            .path(device_path)?
            .build()
            .await?;

        let ap_path = wifi_proxy.active_access_point().await?;

        // check if access point path is valid (not "/" which means no connection)
        if ap_path.as_str() == "/" {
            return Err(anyhow::anyhow!("no active access point"));
        }

        let ap_proxy = AccessPointProxy::builder(conn)
            .path(&ap_path)?
            .build()
            .await?;

        let ssid_bytes = ap_proxy.ssid().await?;
        let strength = ap_proxy.strength().await?;

        // filter out empty SSID
        if ssid_bytes.is_empty() {
            return Err(anyhow::anyhow!("empty SSID"));
        }

        let ssid = String::from_utf8_lossy(&ssid_bytes).to_string();

        // filter out SSIDs that are just whitespace
        if ssid.trim().is_empty() {
            return Err(anyhow::anyhow!("ssid is whitespace only"));
        }

        Ok((ssid, strength))
    }
}
