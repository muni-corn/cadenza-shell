pub mod dbus;
pub mod types;

use futures_lite::StreamExt;
use relm4::{AsyncReducer, AsyncReducible};

use crate::network::{
    dbus::{
        AccessPointProxy, ActiveConnectionProxy, NetworkDeviceProxy, NetworkManagerProxy,
        WirelessDeviceProxy,
    },
    types::{ConnectivityState, DeviceState, DeviceType, State},
};

pub static NETWORK_STATE: AsyncReducer<NetworkInfo> = AsyncReducer::new();

#[derive(Debug, Clone)]
pub struct NetworkInfo {
    pub connected: bool,
    pub state: State,
    pub connectivity: ConnectivityState,
    pub device_type: DeviceType,
    pub wifi_ssid: String,
    pub wifi_strength: u8,
}

impl Default for NetworkInfo {
    fn default() -> Self {
        Self {
            connected: false,
            state: State::Unknown,
            connectivity: ConnectivityState::Unknown,
            device_type: DeviceType::Unknown,
            wifi_ssid: String::new(),
            wifi_strength: 0,
        }
    }
}

#[derive(Debug)]
pub enum NetworkPropertyChange {
    State(State),
    Connectivity(ConnectivityState),
    Primary,
}

impl AsyncReducible for NetworkInfo {
    type Input = NetworkPropertyChange;

    async fn init() -> Self {
        start_network_watcher().await;

        Default::default()
    }

    async fn reduce(&mut self, input: Self::Input) -> bool {
        log::debug!("new network update: {:?}", input);

        match input {
            NetworkPropertyChange::State(state) => self.state = state,
            NetworkPropertyChange::Connectivity(connectivity) => self.connectivity = connectivity,
            NetworkPropertyChange::Primary => match fetch_network_info().await {
                Ok(new_info) => {
                    log::debug!("fetched new network info: {:?}", new_info);
                    *self = new_info;
                }
                Err(e) => {
                    log::error!("couldn't fetch new network info: {e}");
                    return false;
                }
            },
        }

        true
    }
}

pub async fn start_network_watcher() {
    // setup property watching
    if let Err(e) = setup_property_watching().await {
        log::error!("failed to setup network property watching: {}", e);
    }
}

async fn setup_property_watching() -> anyhow::Result<()> {
    let conn = zbus::Connection::system().await?;
    let nm_proxy = NetworkManagerProxy::new(&conn).await?;

    // watch for state changes
    let mut state_stream = nm_proxy.receive_state_changed().await;
    relm4::spawn(async move {
        while let Some(change) = state_stream.next().await {
            if let Ok(new_state) = change
                .get()
                .await
                .inspect_err(|e| log::error!("couldn't get network state change value: {e}"))
            {
                NETWORK_STATE.emit(NetworkPropertyChange::State(new_state));
            }
        }
    });

    // watch for connectivity changes
    let mut connectivity_stream = nm_proxy.receive_connectivity_changed().await;
    relm4::spawn(async move {
        while let Some(change) = connectivity_stream.next().await {
            if let Ok(new_connectivity) = change
                .get()
                .await
                .inspect_err(|e| log::error!("couldn't get network connectivity change value: {e}"))
            {
                NETWORK_STATE.emit(NetworkPropertyChange::Connectivity(new_connectivity));
            }
        }
    });

    // watch for primary connection changes
    let mut primary_connection_stream = nm_proxy.receive_primary_connection_changed().await;
    relm4::spawn(async move {
        while primary_connection_stream.next().await.is_some() {
            NETWORK_STATE.emit(NetworkPropertyChange::Primary);
        }
    });

    Ok(())
}

async fn fetch_network_info() -> anyhow::Result<NetworkInfo> {
    let conn = zbus::Connection::system().await?;
    let nm_proxy = NetworkManagerProxy::new(&conn).await?;

    // get overall state
    let state = nm_proxy.state().await?;
    let is_connected = matches!(
        state,
        State::ConnectedLocal | State::ConnectedSite | State::ConnectedGlobal
    );

    // get connectivity
    let connectivity = nm_proxy.connectivity().await?;

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

                    let device_type = device_proxy.device_type().await?;
                    let device_state = device_proxy.state().await?;

                    // only activated devices are considered active for primary connection
                    if device_state == DeviceState::Activated {
                        info.device_type = device_type;

                        if device_type == DeviceType::Wifi {
                            // get wifi specific info
                            match get_wifi_info(&conn, &device_path).await {
                                Ok(wifi_info) => {
                                    info.wifi_ssid = wifi_info.0;
                                    info.wifi_strength = wifi_info.1;
                                    // wifi info successfully retrieved
                                }
                                Err(e) => {
                                    log::error!("failed to get WiFi info: {}", e);
                                    // try to get at least the interface name as fallback
                                    if let Ok(interface) = device_proxy.interface().await {
                                        info.wifi_ssid = interface;
                                    }
                                }
                            }
                            break; // found our primary wifi connection
                        }
                        // for non-wifi activated devices, set as primary
                        // but continue looking for wifi
                    } else if device_type == DeviceType::Wifi {
                        // even if wifi is not activated, try to get the ssid if connecting
                        if matches!(
                            device_state,
                            DeviceState::Config | DeviceState::IpConfig | DeviceState::IpCheck
                        ) {
                            match get_wifi_info(&conn, &device_path).await {
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
        anyhow::bail!("no active access point");
    }

    let ap_proxy = AccessPointProxy::builder(conn)
        .path(&ap_path)?
        .build()
        .await?;

    let ssid_bytes = ap_proxy.ssid().await?;
    let strength = ap_proxy.strength().await?;

    // filter out empty SSID
    if ssid_bytes.is_empty() {
        anyhow::bail!("empty SSID");
    }

    let ssid = String::from_utf8_lossy(&ssid_bytes).to_string();

    // filter out SSIDs that are just whitespace
    if ssid.trim().is_empty() {
        anyhow::bail!("ssid is whitespace only");
    }

    Ok((ssid, strength))
}
