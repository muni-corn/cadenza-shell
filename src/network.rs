pub mod dbus;
pub mod types;

use futures_lite::StreamExt;
use relm4::SharedState;
use tokio::sync::mpsc::{self, UnboundedReceiver};
use zbus::zvariant::OwnedObjectPath;

use crate::{
    network::{
        dbus::{
            AccessPointProxy, ActiveConnectionProxy, NetworkDeviceProxy, NetworkManagerProxy,
            WirelessDeviceProxy,
        },
        types::{ConnectivityState, DeviceType, State},
    },
    utils::icons::{
        NETWORK_WIFI_ICON_NAMES, NETWORK_WIRED_CONNECTED, NETWORK_WIRED_DISABLED,
        percentage_to_icon_from_list,
    },
};

pub static NETWORK_STATE: SharedState<NetworkInfo> = SharedState::new();

#[derive(Debug, Clone)]
pub struct NetworkInfo {
    pub connection_state: State,
    pub connectivity: ConnectivityState,
    pub specific_info: Option<SpecificNetworkInfo>,
}

impl Default for NetworkInfo {
    fn default() -> Self {
        Self {
            connection_state: State::Unknown,
            connectivity: ConnectivityState::Unknown,
            specific_info: None,
        }
    }
}

impl NetworkInfo {
    pub fn is_asleep(&self) -> bool {
        self.connection_state == State::Asleep
    }

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

#[derive(Clone, Debug)]
pub enum NetworkPropertyChange {
    State(State),
    Connectivity(ConnectivityState),
    Primary(OwnedObjectPath),
}

pub async fn run_network_service() -> Result<(), zbus::Error> {
    let conn = zbus::Connection::system().await?;

    // setup property watching
    match setup_property_watching(&conn).await {
        Ok(mut event_rx) => {
            // fetch initial state before entering event loop
            let nm_proxy = match NetworkManagerProxy::new(&conn).await {
                Ok(p) => p,
                Err(e) => {
                    log::error!("failed to create NM proxy for initial fetch: {e}");
                    return Ok(());
                }
            };
            match nm_proxy.primary_connection().await {
                Ok(path) => match fetch_network_info(&conn, path).await {
                    Ok(info) => {
                        log::debug!("initial network info: {:?}", info);
                        *NETWORK_STATE.write() = info;
                    }
                    Err(e) => log::warn!("couldn't fetch initial network info: {e}"),
                },
                Err(e) => log::warn!("couldn't get primary connection for initial fetch: {e}"),
            }

            // listen for updates to modify network state
            while let Some(event) = event_rx.recv().await {
                match event {
                    NetworkPropertyChange::State(state) => {
                        NETWORK_STATE.write().connection_state = state;
                        // when transitioning to a connected state, re-fetch full info
                        // to update specific_info (SSID, device type, etc.)
                        if matches!(
                            state,
                            State::ConnectedLocal | State::ConnectedSite | State::ConnectedGlobal
                        ) {
                            match nm_proxy.primary_connection().await {
                                Ok(path) => match fetch_network_info(&conn, path).await {
                                    Ok(new_info) => {
                                        log::debug!(
                                            "re-fetched network info after state change: {:?}",
                                            new_info
                                        );
                                        *NETWORK_STATE.write() = new_info;
                                    }
                                    Err(e) => {
                                        log::error!(
                                            "couldn't re-fetch network info after state change: {e}"
                                        );
                                    }
                                },
                                Err(e) => {
                                    log::error!(
                                        "couldn't get primary connection for state re-fetch: {e}"
                                    );
                                }
                            }
                        }
                    }
                    NetworkPropertyChange::Connectivity(connectivity) => {
                        NETWORK_STATE.write().connectivity = connectivity
                    }
                    NetworkPropertyChange::Primary(path) => {
                        match fetch_network_info(&conn, path).await {
                            Ok(new_info) => {
                                log::debug!("fetched new network info: {:?}", new_info);
                                *NETWORK_STATE.write() = new_info;
                            }
                            Err(e) => {
                                log::error!("couldn't fetch new network info: {e}");
                            }
                        }
                    }
                }
            }
            log::warn!("network service has stopped receiving events");
        }
        Err(e) => {
            log::error!("failed to setup network property watching: {}", e);
        }
    }

    Ok(())
}

async fn setup_property_watching(
    conn: &zbus::Connection,
) -> anyhow::Result<UnboundedReceiver<NetworkPropertyChange>> {
    let nm_proxy = NetworkManagerProxy::new(conn).await?;

    let (event_tx, event_rx) = mpsc::unbounded_channel::<NetworkPropertyChange>();

    // watch for state changes
    let event_tx_clone = event_tx.clone();
    let mut state_stream = nm_proxy.receive_state_changed().await;
    relm4::spawn(async move {
        while let Some(change) = state_stream.next().await {
            if let Ok(new_state) = change
                .get()
                .await
                .inspect_err(|e| log::error!("couldn't get network state change value: {e}"))
            {
                event_tx_clone
                    .clone()
                    .send(NetworkPropertyChange::State(new_state))
                    .unwrap_or_else(|e| log::error!("couldn't send state change: {e}"));
            }
        }
        log::warn!("stream for network state changes has closed");
    });

    // watch for connectivity changes
    let mut connectivity_stream = nm_proxy.receive_connectivity_changed().await;
    let event_tx_clone = event_tx.clone();
    relm4::spawn(async move {
        while let Some(change) = connectivity_stream.next().await {
            if let Ok(new_connectivity) = change
                .get()
                .await
                .inspect_err(|e| log::error!("couldn't get network connectivity change value: {e}"))
            {
                event_tx_clone
                    .send(NetworkPropertyChange::Connectivity(new_connectivity))
                    .unwrap_or_else(|e| log::error!("couldn't send connectivity change: {e}"));
            }
        }
        log::warn!("stream for connectivity state changes has closed");
    });

    // watch for primary connection changes
    let mut primary_connection_stream = nm_proxy.receive_primary_connection_changed().await;
    relm4::spawn(async move {
        while let Some(change) = primary_connection_stream.next().await {
            if let Ok(new_primary_connection_path) = change
                .get()
                .await
                .inspect_err(|e| log::error!("couldn't get primary connection change value: {e}"))
            {
                event_tx
                    .send(NetworkPropertyChange::Primary(new_primary_connection_path))
                    .unwrap_or_else(|e| {
                        log::error!("couldn't send primary connection path change: {e}")
                    });
            }
        }
        log::warn!("stream for primary connection state changes has closed");
    });

    Ok(event_rx)
}

async fn fetch_network_info(
    conn: &zbus::Connection,
    primary_connection_path: OwnedObjectPath,
) -> anyhow::Result<NetworkInfo> {
    let nm_proxy = NetworkManagerProxy::new(conn).await?;

    // get overall state
    let connection_state = nm_proxy.state().await?;

    // get connectivity
    let connectivity = nm_proxy.connectivity().await?;

    let is_connected = matches!(
        connection_state,
        State::ConnectedLocal | State::ConnectedSite | State::ConnectedGlobal
    );
    if is_connected {
        // get primary connection details
        let active_conn_proxy = ActiveConnectionProxy::builder(conn)
            .path(&primary_connection_path)?
            .build()
            .await?;

        let active_device_paths = active_conn_proxy.devices().await?;

        log::debug!("active network device paths: {:?}", active_device_paths);

        if let Some(device_path) = active_device_paths.first() {
            let device_proxy = NetworkDeviceProxy::builder(conn)
                .path(device_path)?
                .build()
                .await?;

            let device_type = device_proxy.device_type().await?;

            let specific_info = match device_type {
                DeviceType::Ethernet => Some(SpecificNetworkInfo::Wired),
                DeviceType::Wifi => {
                    let wifi_info = get_wifi_info(conn, device_path).await?;
                    Some(SpecificNetworkInfo::WiFi {
                        wifi_ssid: wifi_info.0,
                        wifi_strength: wifi_info.1,
                    })
                }
                _ => None,
            };

            Ok(NetworkInfo {
                connection_state,
                connectivity,
                specific_info,
            })
        } else {
            Ok(NetworkInfo {
                connection_state,
                connectivity,
                specific_info: None,
            })
        }
    } else {
        Ok(NetworkInfo {
            connection_state,
            connectivity,
            specific_info: None,
        })
    }
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

/// Returns an appropriate icon name for the current networking state.
pub fn get_icon(info: &NetworkInfo) -> &str {
    if let State::Disconnected | State::Disconnecting | State::Asleep | State::Unknown =
        info.connection_state
    {
        return NETWORK_WIRED_DISABLED;
    }

    match info.specific_info {
        Some(SpecificNetworkInfo::WiFi { wifi_strength, .. }) => get_strength_icon(wifi_strength),
        _ => NETWORK_WIRED_CONNECTED,
    }
}

pub fn get_strength_icon(strength: u8) -> &'static str {
    percentage_to_icon_from_list(strength as f64 / 100., NETWORK_WIFI_ICON_NAMES)
}
