pub mod dbus;
pub mod types;

use futures_lite::StreamExt;
use relm4::SharedState;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use zbus::zvariant::OwnedObjectPath;

use crate::{
    network::{
        dbus::{
            AccessPointProxy, ActiveConnectionProxy, NetworkDeviceProxy, NetworkManagerProxy,
            WirelessDeviceProxy,
        },
        types::{ConnectivityState, DeviceType, State},
    },
    sleep_monitor,
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

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub enum NetworkPropertyChange {
    State(State),
    Connectivity(ConnectivityState),
    /// The primary connection object path changed; triggers a full refetch.
    Primary(OwnedObjectPath),
    /// The active access point's signal strength changed.
    Strength(u8),
    /// The system just woke from sleep; triggers a full refetch.
    Wake,
}

pub async fn run_network_service() {
    let Ok((conn, event_tx, mut event_rx)) = setup_property_watching()
        .await
        .inspect_err(|e| log::error!("failed to setup network property watching: {e}"))
    else {
        return;
    };

    // subscribe to system wake events and forward them into the event channel
    let mut wake_rx = sleep_monitor::subscribe_wake();
    let event_tx_wake = event_tx.clone();
    relm4::spawn(async move {
        loop {
            match wake_rx.recv().await {
                Ok(()) => {
                    event_tx_wake
                        .send(NetworkPropertyChange::Wake)
                        .unwrap_or_else(|e| log::error!("couldn't send wake event: {e}"));
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    log::warn!("network wake receiver lagged, missed {n} wake event(s)");
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    // fetch initial state immediately so the tile is correct before any events
    // arrive
    let mut strength_task: Option<tokio::task::AbortHandle> = None;
    if let Err(e) = handle_primary_change(&conn, &event_tx, &mut strength_task).await {
        log::warn!("couldn't fetch initial network state: {e}");
    }

    while let Some(event) = event_rx.recv().await {
        match event {
            NetworkPropertyChange::State(state) => {
                NETWORK_STATE.write().connection_state = state;

                // if we just transitioned to a connected state but specific_info
                // is still None (e.g. the wake refetch ran before NM finished
                // reconnecting and no PrimaryConnection change will fire since
                // we reconnected to the same network), do a full refetch now to
                // populate the missing device details
                if let State::ConnectedLocal | State::ConnectedSite | State::ConnectedGlobal = state
                    && NETWORK_STATE.read().specific_info.is_none()
                {
                    log::debug!("connected state with no device info, refetching network state");
                    if let Err(e) =
                        handle_primary_change(&conn, &event_tx, &mut strength_task).await
                    {
                        log::warn!("couldn't refetch network info after state change: {e}");
                    }
                }
            }
            NetworkPropertyChange::Connectivity(connectivity) => {
                NETWORK_STATE.write().connectivity = connectivity
            }
            NetworkPropertyChange::Primary(_) => {
                if let Err(e) = handle_primary_change(&conn, &event_tx, &mut strength_task).await {
                    log::error!("couldn't handle primary connection change: {e}");
                }
            }
            NetworkPropertyChange::Strength(strength) => {
                let mut state = NETWORK_STATE.write();
                if let Some(SpecificNetworkInfo::WiFi {
                    ref mut wifi_strength,
                    ..
                }) = state.specific_info
                {
                    *wifi_strength = strength;
                }
            }
            NetworkPropertyChange::Wake => {
                log::debug!("system wake: refreshing network state");
                if let Err(e) = handle_primary_change(&conn, &event_tx, &mut strength_task).await {
                    log::warn!("couldn't refresh network state after wake: {e}");
                }
            }
        }
    }
    log::warn!("network service has stopped receiving events");
}

/// Fetches current NM state, updates [`NETWORK_STATE`], and (re)subscribes to
/// access point signal strength changes if on WiFi.
///
/// Any previously running strength subscription task is cancelled first.
async fn handle_primary_change(
    conn: &zbus::Connection,
    event_tx: &UnboundedSender<NetworkPropertyChange>,
    strength_task: &mut Option<tokio::task::AbortHandle>,
) -> anyhow::Result<()> {
    // cancel previous strength subscription before fetching
    if let Some(handle) = strength_task.take() {
        handle.abort();
    }

    let nm_proxy = NetworkManagerProxy::new(conn).await?;
    let primary_path = nm_proxy.primary_connection().await?;
    let (info, ap_path) = fetch_network_info(conn, primary_path).await?;

    log::debug!("fetched network info: {:?}", info);
    *NETWORK_STATE.write() = info;

    // subscribe to strength changes for the new access point
    if let Some(ap_path) = ap_path {
        let tx = event_tx.clone();
        let conn_clone = conn.clone();
        let handle = relm4::spawn(async move {
            subscribe_ap_strength(conn_clone, ap_path, tx).await;
        });
        *strength_task = Some(handle.abort_handle());
    }

    Ok(())
}

/// Sets up D-Bus property watchers for NetworkManager.
///
/// Returns the shared connection, a sender for injecting events (used when
/// spawning the strength subscription task), and the event receiver.
async fn setup_property_watching() -> anyhow::Result<(
    zbus::Connection,
    UnboundedSender<NetworkPropertyChange>,
    UnboundedReceiver<NetworkPropertyChange>,
)> {
    let conn = zbus::Connection::system().await?;
    let nm_proxy = NetworkManagerProxy::new(&conn).await?;

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
    let event_tx_clone = event_tx.clone();
    relm4::spawn(async move {
        while let Some(change) = primary_connection_stream.next().await {
            if let Ok(new_primary_connection_path) = change
                .get()
                .await
                .inspect_err(|e| log::error!("couldn't get primary connection change value: {e}"))
            {
                event_tx_clone
                    .send(NetworkPropertyChange::Primary(new_primary_connection_path))
                    .unwrap_or_else(|e| {
                        log::error!("couldn't send primary connection path change: {e}")
                    });
            }
        }
        log::warn!("stream for primary connection state changes has closed");
    });

    Ok((conn, event_tx, event_rx))
}

/// Fetches full network info for the given primary connection path.
///
/// Returns the `NetworkInfo` and, if connected via WiFi, the active access
/// point object path (for setting up a strength subscription).
async fn fetch_network_info(
    conn: &zbus::Connection,
    primary_connection_path: OwnedObjectPath,
) -> anyhow::Result<(NetworkInfo, Option<OwnedObjectPath>)> {
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

            match device_type {
                DeviceType::Ethernet => Ok((
                    NetworkInfo {
                        connection_state,
                        connectivity,
                        specific_info: Some(SpecificNetworkInfo::Wired),
                    },
                    None,
                )),
                DeviceType::Wifi => {
                    let (ssid, strength, ap_path) = get_wifi_info(conn, device_path).await?;
                    Ok((
                        NetworkInfo {
                            connection_state,
                            connectivity,
                            specific_info: Some(SpecificNetworkInfo::WiFi {
                                wifi_ssid: ssid,
                                wifi_strength: strength,
                            }),
                        },
                        Some(ap_path),
                    ))
                }
                _ => Ok((
                    NetworkInfo {
                        connection_state,
                        connectivity,
                        specific_info: None,
                    },
                    None,
                )),
            }
        } else {
            Ok((
                NetworkInfo {
                    connection_state,
                    connectivity,
                    specific_info: None,
                },
                None,
            ))
        }
    } else {
        Ok((
            NetworkInfo {
                connection_state,
                connectivity,
                specific_info: None,
            },
            None,
        ))
    }
}

/// Returns the SSID, current strength, and object path of the active access
/// point for the given wireless device.
async fn get_wifi_info(
    conn: &zbus::Connection,
    device_path: &zbus::zvariant::OwnedObjectPath,
) -> anyhow::Result<(String, u8, OwnedObjectPath)> {
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

    Ok((ssid, strength, ap_path))
}

/// Subscribes to strength property changes on an access point and forwards
/// them as [`NetworkPropertyChange::Strength`] events.
///
/// This task runs until the stream closes or the task is aborted (e.g. when
/// the primary connection changes or the system sleeps).
async fn subscribe_ap_strength(
    conn: zbus::Connection,
    ap_path: OwnedObjectPath,
    tx: UnboundedSender<NetworkPropertyChange>,
) {
    let builder = match AccessPointProxy::builder(&conn).path(&ap_path) {
        Ok(b) => b,
        Err(e) => {
            log::error!("invalid access point path {ap_path}: {e}");
            return;
        }
    };
    let ap_proxy = match builder.build().await {
        Ok(p) => p,
        Err(e) => {
            log::error!("couldn't build access point proxy for strength subscription: {e}");
            return;
        }
    };

    let mut stream = ap_proxy.receive_strength_changed().await;
    log::debug!("subscribed to strength changes for access point {ap_path}");

    while let Some(change) = stream.next().await {
        if let Ok(strength) = change
            .get()
            .await
            .inspect_err(|e| log::debug!("couldn't get strength value: {e}"))
        {
            tx.send(NetworkPropertyChange::Strength(strength))
                .unwrap_or_else(|e| log::error!("couldn't send strength change: {e}"));
        }
    }

    log::debug!("strength subscription for access point {ap_path} ended");
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
