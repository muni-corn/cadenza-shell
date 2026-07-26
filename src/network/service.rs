use std::time::Duration;

use futures_lite::StreamExt;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use zbus::zvariant::OwnedObjectPath;

use crate::{
    network::{
        dbus::{
            AccessPointProxy, ActiveConnectionProxy, NetworkDeviceProxy, NetworkManagerProxy,
            WirelessDeviceProxy,
        },
        state::{NETWORK_STATE, NetworkInfo, SpecificNetworkInfo},
        types::{DeviceType, State},
    },
    settings, sleep_monitor,
};

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub enum NetworkPropertyChange {
    State(State),
    Connectivity(crate::network::types::ConnectivityState),
    /// The primary connection object path changed; triggers a full refetch.
    Primary(OwnedObjectPath),
    /// The active access point's signal strength changed.
    Strength(u8),
    /// The wireless device's active access point changed (e.g. NetworkManager
    /// roamed to a different BSSID on the same SSID, which doesn't change
    /// `PrimaryConnection`); triggers a full refetch.
    ActiveApChanged,
    /// The wifi radio was turned on or off.
    WifiEnabled(bool),
    /// The system just woke from sleep; triggers a full refetch.
    Wake,
    /// Periodic full refetch, independent of any D-Bus event. Self-heals
    /// state that a missed or dropped signal would otherwise leave stale
    /// forever, the same way battery::watcher's poll interval does.
    Reconcile,
    /// NetworkManager (re)appeared on the bus under a new unique name (e.g.
    /// `systemctl restart NetworkManager`, or it starting after us). Existing
    /// property-change subscriptions are bound to the previous owner's
    /// unique name and go silent when it disappears, so they must be torn
    /// down and re-established against the new owner.
    NetworkManagerRestarted,
}

/// Tracks the abort handles for the top-level NetworkManager property
/// watchers, so they can be cancelled and respawned when NetworkManager
/// itself restarts under a new unique bus name.
struct PropertyWatcherTasks {
    state: tokio::task::AbortHandle,
    connectivity: tokio::task::AbortHandle,
    primary: tokio::task::AbortHandle,
    wireless_enabled: tokio::task::AbortHandle,
}

impl PropertyWatcherTasks {
    fn abort_all(&self) {
        self.state.abort();
        self.connectivity.abort();
        self.primary.abort();
        self.wireless_enabled.abort();
    }
}

/// Tracks the abort handles for the per-connection subscription tasks that
/// are cancelled and respawned every time the primary connection changes.
#[derive(Default)]
struct SubscriptionTasks {
    strength: Option<tokio::task::AbortHandle>,
    active_ap: Option<tokio::task::AbortHandle>,
}

impl SubscriptionTasks {
    fn abort_all(&mut self) {
        if let Some(handle) = self.strength.take() {
            handle.abort();
        }
        if let Some(handle) = self.active_ap.take() {
            handle.abort();
        }
    }
}

pub async fn run_network_service() {
    let Ok((conn, event_tx, mut event_rx, mut property_tasks)) = setup_property_watching()
        .await
        .inspect_err(|e| tracing::error!("failed to setup network property watching: {e}"))
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
                        .unwrap_or_else(|e| tracing::error!("couldn't send wake event: {e}"));
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!("network wake receiver lagged, missed {n} wake event(s)");
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    // periodically reconcile full state so a missed or dropped D-Bus signal
    // never leaves the tile stale indefinitely
    let event_tx_reconcile = event_tx.clone();
    relm4::spawn(async move {
        let interval_secs = settings::get_config().network.reconcile_interval;
        let mut interval = tokio::time::interval(Duration::from_secs(interval_secs));
        // skip the first tick, which fires immediately; we already fetch
        // initial state separately below
        interval.tick().await;
        loop {
            interval.tick().await;
            event_tx_reconcile
                .send(NetworkPropertyChange::Reconcile)
                .unwrap_or_else(|e| tracing::error!("couldn't send reconcile tick: {e}"));
        }
    });

    // fetch initial state immediately so the tile is correct before any events
    // arrive
    let mut tasks = SubscriptionTasks::default();
    if let Err(e) = handle_primary_change(&conn, &event_tx, &mut tasks).await {
        tracing::warn!("couldn't fetch initial network state: {e}");
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
                    tracing::debug!(
                        "connected state with no device info, refetching network state"
                    );
                    if let Err(e) = handle_primary_change(&conn, &event_tx, &mut tasks).await {
                        tracing::warn!("couldn't refetch network info after state change: {e}");
                    }
                }
            }
            NetworkPropertyChange::Connectivity(connectivity) => {
                NETWORK_STATE.write().connectivity = connectivity
            }
            NetworkPropertyChange::Primary(_) => {
                if let Err(e) = handle_primary_change(&conn, &event_tx, &mut tasks).await {
                    tracing::error!("couldn't handle primary connection change: {e}");
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
            NetworkPropertyChange::ActiveApChanged => {
                tracing::debug!("active access point changed, refetching network state");
                if let Err(e) = handle_primary_change(&conn, &event_tx, &mut tasks).await {
                    tracing::warn!("couldn't refetch network state after roaming: {e}");
                }
            }
            NetworkPropertyChange::WifiEnabled(enabled) => {
                NETWORK_STATE.write().wifi_enabled = enabled;
            }
            NetworkPropertyChange::Wake => {
                tracing::debug!("system wake: refreshing network state");
                if let Err(e) = handle_primary_change(&conn, &event_tx, &mut tasks).await {
                    tracing::warn!("couldn't refresh network state after wake: {e}");
                }
            }
            NetworkPropertyChange::Reconcile => {
                tracing::debug!("periodic reconcile: refreshing network state");
                if let Err(e) = handle_primary_change(&conn, &event_tx, &mut tasks).await {
                    tracing::warn!("couldn't reconcile network state: {e}");
                }
            }
            NetworkPropertyChange::NetworkManagerRestarted => {
                tracing::info!(
                    "networkmanager reappeared on the bus, re-establishing subscriptions"
                );
                property_tasks.abort_all();
                match spawn_property_watchers(&conn, &event_tx).await {
                    Ok(new_tasks) => property_tasks = new_tasks,
                    Err(e) => {
                        tracing::error!("couldn't re-establish property watchers: {e}");
                    }
                }
                if let Err(e) = handle_primary_change(&conn, &event_tx, &mut tasks).await {
                    tracing::warn!("couldn't refetch network state after restart: {e}");
                }
            }
        }
    }
    tracing::warn!("network service has stopped receiving events");
}

/// Fetches current NM state, updates [`NETWORK_STATE`], and (re)subscribes to
/// access point signal strength changes and active-access-point changes if
/// on WiFi.
///
/// Any previously running subscription tasks are cancelled first.
async fn handle_primary_change(
    conn: &zbus::Connection,
    event_tx: &UnboundedSender<NetworkPropertyChange>,
    tasks: &mut SubscriptionTasks,
) -> anyhow::Result<()> {
    tasks.abort_all();

    let nm_proxy = NetworkManagerProxy::new(conn).await?;
    let primary_path = nm_proxy.primary_connection().await?;
    let (info, ap_path, wifi_device_path) = fetch_network_info(conn, primary_path).await?;

    tracing::debug!("fetched network info: {:?}", info);
    *NETWORK_STATE.write() = info;

    // subscribe to strength changes for the new access point
    if let Some(ap_path) = ap_path {
        let tx = event_tx.clone();
        let conn_clone = conn.clone();
        let handle = relm4::spawn(async move {
            subscribe_ap_strength(conn_clone, ap_path, tx).await;
        });
        tasks.strength = Some(handle.abort_handle());
    }

    // subscribe to active access point changes on the wireless device, so
    // roaming to a different BSSID (which doesn't change PrimaryConnection)
    // still triggers a refetch instead of leaving strength/SSID frozen on
    // the now-disappeared access point object
    if let Some(device_path) = wifi_device_path {
        let tx = event_tx.clone();
        let conn_clone = conn.clone();
        let handle = relm4::spawn(async move {
            subscribe_active_ap(conn_clone, device_path, tx).await;
        });
        tasks.active_ap = Some(handle.abort_handle());
    }

    Ok(())
}

/// Sets up D-Bus property watchers for NetworkManager.
///
/// Returns the shared connection, a sender for injecting events (used when
/// spawning the strength subscription task), the event receiver, and the
/// abort handles for the spawned property watcher tasks.
async fn setup_property_watching() -> anyhow::Result<(
    zbus::Connection,
    UnboundedSender<NetworkPropertyChange>,
    UnboundedReceiver<NetworkPropertyChange>,
    PropertyWatcherTasks,
)> {
    let conn = zbus::Connection::system().await?;
    let (event_tx, event_rx) = mpsc::unbounded_channel::<NetworkPropertyChange>();

    let property_tasks = spawn_property_watchers(&conn, &event_tx).await?;
    spawn_name_owner_watcher(&conn, &event_tx).await;

    Ok((conn, event_tx, event_rx, property_tasks))
}

/// Spawns the top-level NetworkManager property watchers (state,
/// connectivity, primary connection, wifi enabled), forwarding changes into
/// `event_tx`.
///
/// Callable both at startup and again after a `NetworkManagerRestarted`
/// event, since the resulting signal subscriptions are bound to whichever
/// unique name currently owns the `org.freedesktop.NetworkManager` bus name
/// at subscribe time.
async fn spawn_property_watchers(
    conn: &zbus::Connection,
    event_tx: &UnboundedSender<NetworkPropertyChange>,
) -> anyhow::Result<PropertyWatcherTasks> {
    let nm_proxy = NetworkManagerProxy::new(conn).await?;

    // watch for state changes
    let event_tx_clone = event_tx.clone();
    let mut state_stream = nm_proxy.receive_state_changed().await;
    let state =
        relm4::spawn(async move {
            while let Some(change) = state_stream.next().await {
                if let Ok(new_state) = change.get().await.inspect_err(|e| {
                    tracing::error!("couldn't get network state change value: {e}")
                }) {
                    event_tx_clone
                        .clone()
                        .send(NetworkPropertyChange::State(new_state))
                        .unwrap_or_else(|e| tracing::error!("couldn't send state change: {e}"));
                }
            }
            tracing::warn!("stream for network state changes has closed");
        })
        .abort_handle();

    // watch for connectivity changes
    let mut connectivity_stream = nm_proxy.receive_connectivity_changed().await;
    let event_tx_clone = event_tx.clone();
    let connectivity = relm4::spawn(async move {
        while let Some(change) = connectivity_stream.next().await {
            if let Ok(new_connectivity) = change.get().await.inspect_err(|e| {
                tracing::error!("couldn't get network connectivity change value: {e}")
            }) {
                event_tx_clone
                    .send(NetworkPropertyChange::Connectivity(new_connectivity))
                    .unwrap_or_else(|e| tracing::error!("couldn't send connectivity change: {e}"));
            }
        }
        tracing::warn!("stream for connectivity state changes has closed");
    })
    .abort_handle();

    // watch for primary connection changes
    let mut primary_connection_stream = nm_proxy.receive_primary_connection_changed().await;
    let event_tx_clone = event_tx.clone();
    let primary = relm4::spawn(async move {
        while let Some(change) = primary_connection_stream.next().await {
            if let Ok(new_primary_connection_path) = change.get().await.inspect_err(|e| {
                tracing::error!("couldn't get primary connection change value: {e}")
            }) {
                event_tx_clone
                    .send(NetworkPropertyChange::Primary(new_primary_connection_path))
                    .unwrap_or_else(|e| {
                        tracing::error!("couldn't send primary connection path change: {e}")
                    });
            }
        }
        tracing::warn!("stream for primary connection state changes has closed");
    })
    .abort_handle();

    // watch for the wifi radio being switched on or off
    let mut wireless_enabled_stream = nm_proxy.receive_wireless_enabled_changed().await;
    let event_tx_clone = event_tx.clone();
    let wireless_enabled = relm4::spawn(async move {
        while let Some(change) = wireless_enabled_stream.next().await {
            if let Ok(enabled) = change
                .get()
                .await
                .inspect_err(|e| tracing::error!("couldn't get wireless enabled value: {e}"))
            {
                event_tx_clone
                    .send(NetworkPropertyChange::WifiEnabled(enabled))
                    .unwrap_or_else(|e| tracing::error!("couldn't send wifi enabled change: {e}"));
            }
        }
        tracing::warn!("stream for wireless enabled changes has closed");
    })
    .abort_handle();

    Ok(PropertyWatcherTasks {
        state,
        connectivity,
        primary,
        wireless_enabled,
    })
}

/// Watches for NetworkManager appearing under a new unique bus name (a
/// restart, or it starting up after us) and forwards
/// [`NetworkPropertyChange::NetworkManagerRestarted`] when it does.
///
/// This task itself never needs to be respawned: it's subscribed against
/// `org.freedesktop.DBus`, the bus daemon itself, which does not restart
/// out from under us the way NetworkManager can.
async fn spawn_name_owner_watcher(
    conn: &zbus::Connection,
    event_tx: &UnboundedSender<NetworkPropertyChange>,
) {
    let dbus_proxy = match zbus::fdo::DBusProxy::new(conn).await {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("couldn't create DBus proxy for name owner watching: {e}");
            return;
        }
    };

    let mut stream = match dbus_proxy.receive_name_owner_changed().await {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("couldn't subscribe to NameOwnerChanged: {e}");
            return;
        }
    };

    let event_tx = event_tx.clone();
    relm4::spawn(async move {
        while let Some(signal) = stream.next().await {
            let Ok(args) = signal
                .args()
                .inspect_err(|e| tracing::error!("couldn't parse NameOwnerChanged args: {e}"))
            else {
                continue;
            };

            if args.name.as_str() != "org.freedesktop.NetworkManager" {
                continue;
            }

            // a non-empty new_owner means NetworkManager is now owned by
            // some process (either it just started, or it restarted under a
            // new unique name); either way our existing subscriptions need
            // to be re-established against the current owner
            if args.new_owner.is_some() {
                event_tx
                    .send(NetworkPropertyChange::NetworkManagerRestarted)
                    .unwrap_or_else(|e| {
                        tracing::error!("couldn't send networkmanager restart event: {e}")
                    });
            } else {
                tracing::warn!("networkmanager disappeared from the bus");
            }
        }
        tracing::warn!("stream for NameOwnerChanged has closed");
    });
}

/// Fetches full network info for the given primary connection path.
///
/// Returns the `NetworkInfo` and, if connected via WiFi, the active access
/// point object path (for setting up a strength subscription) and the
/// wireless device's object path (for setting up an active-access-point
/// subscription).
///
/// Only fails if the overall connection state and connectivity themselves
/// can't be read (i.e. NetworkManager itself is unreachable). If the
/// device-specific details (which device, which access point) can't be
/// determined - e.g. because `ActiveAccessPoint` hasn't been populated yet
/// mid-association - `specific_info` degrades to `None` rather than
/// discarding the whole fetch, so [`NETWORK_STATE`] is never left stale
/// after a transient read failure.
async fn fetch_network_info(
    conn: &zbus::Connection,
    primary_connection_path: OwnedObjectPath,
) -> anyhow::Result<(
    NetworkInfo,
    Option<OwnedObjectPath>,
    Option<OwnedObjectPath>,
)> {
    let nm_proxy = NetworkManagerProxy::new(conn).await?;

    let connection_state = nm_proxy.state().await?;
    let connectivity = nm_proxy.connectivity().await?;
    // WirelessEnabled should basically never fail to read alongside State
    // and Connectivity, but fall back to "enabled" defensively rather than
    // abort the whole fetch over a single missing property.
    let wifi_enabled = nm_proxy
        .wireless_enabled()
        .await
        .inspect_err(|e| tracing::warn!("couldn't read wireless enabled state: {e}"))
        .unwrap_or(true);

    let is_connected = matches!(
        connection_state,
        State::ConnectedLocal | State::ConnectedSite | State::ConnectedGlobal
    );

    let (specific_info, ap_path, wifi_device_path) = if is_connected {
        fetch_specific_info(conn, &primary_connection_path)
            .await
            .unwrap_or_else(|e| {
                tracing::debug!("couldn't fetch device-specific network info: {e}");
                (None, None, None)
            })
    } else {
        (None, None, None)
    };

    Ok((
        NetworkInfo {
            connection_state,
            connectivity,
            specific_info,
            wifi_enabled,
        },
        ap_path,
        wifi_device_path,
    ))
}

/// Fetches the connected device's type-specific info (wired or wifi).
///
/// Returns `Ok((None, None, None))` when the primary connection has no
/// associated device yet, or the device is of a type we don't render
/// distinctly (e.g. mobile broadband). The third element is the wireless
/// device's own object path, present whenever the connected device is wifi
/// regardless of whether an access point could be read.
async fn fetch_specific_info(
    conn: &zbus::Connection,
    primary_connection_path: &OwnedObjectPath,
) -> anyhow::Result<(
    Option<SpecificNetworkInfo>,
    Option<OwnedObjectPath>,
    Option<OwnedObjectPath>,
)> {
    let active_conn_proxy = ActiveConnectionProxy::builder(conn)
        .path(primary_connection_path)?
        .build()
        .await?;

    let active_device_paths = active_conn_proxy.devices().await?;
    tracing::debug!("active network device paths: {:?}", active_device_paths);

    let Some(device_path) = active_device_paths.first() else {
        return Ok((None, None, None));
    };

    let device_proxy = NetworkDeviceProxy::builder(conn)
        .path(device_path)?
        .build()
        .await?;

    match device_proxy.device_type().await? {
        DeviceType::Ethernet => Ok((Some(SpecificNetworkInfo::Wired), None, None)),
        DeviceType::Wifi => {
            // always report the wifi device path so the caller can subscribe
            // to ActiveAccessPoint changes even if there's no access point to
            // read yet (e.g. mid-association); otherwise we'd never notice
            // once one becomes available until an unrelated event fires
            match get_wifi_info(conn, device_path).await {
                Ok((ssid, strength, ap_path)) => Ok((
                    Some(SpecificNetworkInfo::WiFi {
                        wifi_ssid: ssid,
                        wifi_strength: strength,
                    }),
                    Some(ap_path),
                    Some(device_path.clone()),
                )),
                Err(e) => {
                    tracing::debug!("couldn't fetch wifi access point info: {e}");
                    Ok((None, None, Some(device_path.clone())))
                }
            }
        }
        _ => Ok((None, None, None)),
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
            tracing::error!("invalid access point path {ap_path}: {e}");
            return;
        }
    };
    let ap_proxy = match builder.build().await {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("couldn't build access point proxy for strength subscription: {e}");
            return;
        }
    };

    let mut stream = ap_proxy.receive_strength_changed().await;
    tracing::debug!("subscribed to strength changes for access point {ap_path}");

    while let Some(change) = stream.next().await {
        if let Ok(strength) = change
            .get()
            .await
            .inspect_err(|e| tracing::debug!("couldn't get strength value: {e}"))
        {
            tx.send(NetworkPropertyChange::Strength(strength))
                .unwrap_or_else(|e| tracing::error!("couldn't send strength change: {e}"));
        }
    }

    tracing::debug!("strength subscription for access point {ap_path} ended");
}

/// Subscribes to active-access-point changes on a wireless device and
/// forwards them as [`NetworkPropertyChange::ActiveApChanged`] events.
///
/// This catches roaming to a different BSSID on the same SSID, which does
/// not change `PrimaryConnection` and would otherwise leave the strength
/// subscription pointed at an access point object that has left the bus.
///
/// This task runs until the stream closes or the task is aborted (e.g. when
/// the primary connection changes or the system sleeps).
async fn subscribe_active_ap(
    conn: zbus::Connection,
    device_path: OwnedObjectPath,
    tx: UnboundedSender<NetworkPropertyChange>,
) {
    let builder = match WirelessDeviceProxy::builder(&conn).path(&device_path) {
        Ok(b) => b,
        Err(e) => {
            tracing::error!("invalid wireless device path {device_path}: {e}");
            return;
        }
    };
    let wifi_proxy = match builder.build().await {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("couldn't build wireless device proxy for active AP subscription: {e}");
            return;
        }
    };

    let mut stream = wifi_proxy.receive_active_access_point_changed().await;
    tracing::debug!("subscribed to active access point changes for device {device_path}");

    while stream.next().await.is_some() {
        tx.send(NetworkPropertyChange::ActiveApChanged)
            .unwrap_or_else(|e| tracing::error!("couldn't send active AP change: {e}"));
    }

    tracing::debug!("active access point subscription for device {device_path} ended");
}
