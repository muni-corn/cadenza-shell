mod connect_outcome;
mod fetch;
mod handlers;
mod watchers;

use std::time::Duration;

use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

use self::{
    fetch::fetch_network_info,
    handlers::{ensure_scan_watcher, handle_command, refresh_wifi_access_points},
    watchers::{
        spawn_name_owner_watcher, spawn_property_watchers, subscribe_active_ap,
        subscribe_ap_strength,
    },
};
use crate::{
    network::{
        commands::{self, NetworkCommand},
        dbus::NetworkManagerProxy,
        state::{NETWORK_STATE, NetworkInfo, SpecificNetworkInfo, WIFI_SCAN_STATE},
        types::State,
    },
    settings, sleep_monitor,
};

#[derive(Clone, Debug)]
pub enum NetworkPropertyChange {
    State(State),
    Connectivity(crate::network::types::ConnectivityState),
    /// The primary connection object path changed; triggers a full refetch.
    Primary,
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
    /// `systemctl restart NetworkManager`, or it starting after us). The
    /// existing proxies filled their property caches from the previous
    /// owner and can't refill them, so their property streams stop
    /// producing values; the watchers are rebuilt against the new owner.
    NetworkManagerRestarted,
    /// NetworkManager left the bus without a replacement.
    NetworkManagerVanished,
    /// A command issued by the UI (via `network::commands`).
    Command(NetworkCommand),
    /// The wifi device finished a scan (its `LastScan` advanced).
    ScanCompleted,
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

    // install the command sender so free functions in network::commands can
    // push commands, and forward them into the same event channel as
    // everything else
    let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel::<NetworkCommand>();
    if commands::install_command_sender(cmd_tx).is_err() {
        tracing::warn!("network service started more than once; extra instance exiting");
        return;
    }
    let event_tx_cmd = event_tx.clone();
    relm4::spawn(async move {
        while let Some(cmd) = cmd_rx.recv().await {
            event_tx_cmd
                .send(NetworkPropertyChange::Command(cmd))
                .unwrap_or_else(|e| tracing::error!("couldn't forward network command: {e}"));
        }
        tracing::warn!("network command channel closed");
    });

    // fetch initial state immediately so the tile is correct before any events
    // arrive
    let mut tasks = SubscriptionTasks::default();
    let mut scan_watcher = None;
    // reconciling while NetworkManager is gone would only fail and log
    let mut nm_present = true;
    if let Err(e) = handle_primary_change(&conn, &event_tx, &mut tasks).await {
        tracing::warn!("couldn't fetch initial network state: {e}");
    }

    while let Some(event) = event_rx.recv().await {
        match event {
            NetworkPropertyChange::State(state) => {
                NETWORK_STATE.write().connection_state = state;

                // refetch as soon as a connection starts activating, so
                // device_kind (and thus the wifi/wired acquiring icon) is
                // known right away - ActivatingConnection has no dedicated
                // watcher of its own, unlike PrimaryConnection
                if state == State::Connecting {
                    tracing::debug!("connecting: refetching network state for device kind");
                    if let Err(e) = handle_primary_change(&conn, &event_tx, &mut tasks).await {
                        tracing::warn!("couldn't refetch network state while connecting: {e}");
                    }
                }

                // if we just transitioned to a connected state but
                // specific_info is still None (e.g. the wake refetch ran before
                // NM finished reconnecting and no PrimaryConnection change will
                // fire since we reconnected to the same network), do a full
                // refetch now to populate the missing device details
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
            NetworkPropertyChange::Primary => {
                if let Err(e) = handle_primary_change(&conn, &event_tx, &mut tasks).await {
                    tracing::error!("couldn't handle primary connection change: {e}");
                }
            }
            NetworkPropertyChange::Strength(strength) => {
                let mut state = NETWORK_STATE.write();
                if let Some(wifi) = &mut state.wifi {
                    wifi.strength = strength;
                }
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
                if !nm_present {
                    continue;
                }
                tracing::debug!("periodic reconcile: refreshing network state");
                if let Err(e) = handle_primary_change(&conn, &event_tx, &mut tasks).await {
                    tracing::warn!("couldn't reconcile network state: {e}");
                }
            }
            NetworkPropertyChange::NetworkManagerVanished => {
                tracing::warn!("networkmanager disappeared from the bus");
                nm_present = false;
                tasks.abort_all();
                *NETWORK_STATE.write() = NetworkInfo::default();
                *WIFI_SCAN_STATE.write() = Default::default();
            }
            NetworkPropertyChange::NetworkManagerRestarted => {
                tracing::info!(
                    "networkmanager reappeared on the bus, re-establishing subscriptions"
                );
                nm_present = true;
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
            NetworkPropertyChange::Command(cmd) => {
                // results only arrive once LastScan advances, so make sure
                // something is listening before a scan is requested
                if matches!(cmd, NetworkCommand::Scan) {
                    ensure_scan_watcher(&conn, &event_tx, &mut scan_watcher).await;
                }

                let changes_saved_networks = matches!(
                    cmd,
                    NetworkCommand::Connect { .. }
                        | NetworkCommand::Disconnect
                        | NetworkCommand::Forget(_)
                );
                handle_command(&conn, cmd).await;

                // saved and connected markers in the menu changed
                if changes_saved_networks {
                    refresh_wifi_access_points(&conn).await;
                }
            }
            NetworkPropertyChange::ScanCompleted => {
                tracing::debug!("wifi scan completed, refreshing access points");
                refresh_wifi_access_points(&conn).await;
            }
        }
    }
    tracing::warn!("network service has stopped receiving events");
}

/// Fetches current NM state, updates [`NETWORK_STATE`], and (re)subscribes to
/// access point signal strength changes and active-access-point changes if
/// on WiFi.
///
/// Previously running subscription tasks are replaced once the fetch
/// succeeds; a failed fetch leaves them running.
async fn handle_primary_change(
    conn: &zbus::Connection,
    event_tx: &UnboundedSender<NetworkPropertyChange>,
    tasks: &mut SubscriptionTasks,
) -> anyhow::Result<()> {
    let nm_proxy = NetworkManagerProxy::new(conn).await?;
    let primary_path = nm_proxy.primary_connection().await?;
    let (info, ap_path, wifi_device_path) = fetch_network_info(conn, primary_path).await?;

    tasks.abort_all();

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
