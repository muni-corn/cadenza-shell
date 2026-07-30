use std::{collections::HashMap, time::Duration};

use futures_lite::StreamExt;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use zbus::zvariant::{ObjectPath, OwnedObjectPath, Value};

use crate::{
    network::{
        commands::{self, NetworkCommand},
        dbus::{
            AccessPointProxy, ActiveConnectionProxy, NetworkDeviceProxy, NetworkManagerProxy,
            SettingsConnectionProxy, WirelessDeviceProxy,
        },
        events::{self, ConnectFailureReason, NetworkEvent},
        scan,
        state::{DeviceKind, NETWORK_STATE, NetworkInfo, SpecificNetworkInfo, WIFI_SCAN_STATE},
        types::{ApSecurity, DeviceState, DeviceStateReason, DeviceType, State},
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
    /// A command issued by the UI (via `network::commands`).
    Command(NetworkCommand),
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
            NetworkPropertyChange::Command(cmd) => {
                handle_command(&conn, cmd).await;
            }
        }
    }
    tracing::warn!("network service has stopped receiving events");
}

/// Dispatches a single UI-issued command to its D-Bus handler, logging on
/// failure. Failures are not fatal to the service; the user can simply
/// retry from the menu.
async fn handle_command(conn: &zbus::Connection, cmd: NetworkCommand) {
    match cmd {
        NetworkCommand::SetWifiEnabled(enabled) => {
            if let Err(e) = handle_set_wifi_enabled(conn, enabled).await {
                tracing::error!("couldn't set wifi enabled to {enabled}: {e}");
            }
        }
        NetworkCommand::Scan => {
            if let Err(e) = handle_scan(conn).await {
                tracing::warn!("couldn't request wifi scan: {e}");
            }
        }
        NetworkCommand::Connect {
            ssid,
            security,
            password,
        } => {
            if let Err(e) = handle_connect(conn, &ssid, security, password.as_deref()).await {
                tracing::error!("couldn't connect to {ssid}: {e}");
            }
        }
        NetworkCommand::Disconnect => {
            if let Err(e) = handle_disconnect(conn).await {
                tracing::error!("couldn't disconnect: {e}");
            }
        }
        NetworkCommand::Forget(connection_path) => {
            if let Err(e) = handle_forget(conn, &connection_path).await {
                tracing::error!("couldn't forget connection {connection_path}: {e}");
            }
        }
    }
}

/// Finds the first wifi-capable network device, regardless of its current
/// connection state (unlike the wireless device path tracked in
/// `SubscriptionTasks`, which is only populated once a wifi connection is
/// already active).
async fn find_wireless_device(conn: &zbus::Connection) -> anyhow::Result<OwnedObjectPath> {
    let nm_proxy = NetworkManagerProxy::new(conn).await?;
    let device_paths = nm_proxy.get_devices().await?;

    for path in device_paths {
        let Ok(device_proxy) = NetworkDeviceProxy::builder(conn).path(&path)?.build().await else {
            continue;
        };
        if let Ok(DeviceType::Wifi) = device_proxy.device_type().await {
            return Ok(path);
        }
    }

    anyhow::bail!("no wireless device found")
}

async fn handle_set_wifi_enabled(conn: &zbus::Connection, enabled: bool) -> anyhow::Result<()> {
    let nm_proxy = NetworkManagerProxy::new(conn).await?;
    nm_proxy.set_wireless_enabled(enabled).await?;
    Ok(())
}

/// Requests a scan, then refetches and publishes the access point list.
///
/// `RequestScan` only asks NetworkManager to *start* scanning; it doesn't
/// wait for completion, so the immediate refetch below will usually still
/// show pre-scan results. The menu re-requests a scan periodically while
/// open (see a following commit) to eventually pick up fresh results once
/// `LastScan` advances.
async fn handle_scan(conn: &zbus::Connection) -> anyhow::Result<()> {
    WIFI_SCAN_STATE.write().scanning = true;

    let device_path = find_wireless_device(conn).await?;
    let wifi_proxy = WirelessDeviceProxy::builder(conn)
        .path(&device_path)?
        .build()
        .await?;

    let scan_result = wifi_proxy.request_scan(HashMap::new()).await;

    if let Err(e) = &scan_result {
        tracing::warn!("RequestScan failed: {e}");
    }

    if let Err(e) = refresh_access_points(conn, &device_path).await {
        tracing::warn!("couldn't refresh access points after scan request: {e}");
    }

    WIFI_SCAN_STATE.write().scanning = false;

    scan_result.map_err(anyhow::Error::from)
}

/// Refetches the access point list and `LastScan` timestamp for the given
/// wireless device and publishes them to [`WIFI_SCAN_STATE`].
async fn refresh_access_points(
    conn: &zbus::Connection,
    device_path: &OwnedObjectPath,
) -> anyhow::Result<()> {
    let active_ssid = NETWORK_STATE.read().wifi_ssid();
    let access_points = scan::fetch_access_points(conn, device_path, active_ssid.as_deref())
        .await
        .inspect_err(|e| tracing::warn!("couldn't fetch access points: {e}"))
        .unwrap_or_default();

    let wifi_proxy = WirelessDeviceProxy::builder(conn)
        .path(device_path)?
        .build()
        .await?;
    let last_scan_ms = wifi_proxy.last_scan().await.unwrap_or(-1);

    let mut state = WIFI_SCAN_STATE.write();
    state.access_points = access_points;
    state.last_scan_ms = last_scan_ms;

    Ok(())
}

/// Connects to a wifi network by SSID, letting NetworkManager reuse a saved
/// connection profile if one matches, or create a new one otherwise.
///
/// Passing a partial settings dict (just the SSID, plus security settings if
/// a password was given) rather than a fully-specified connection is
/// intentional: `AddAndActivateConnection` completes it against any existing
/// matching profile, so this same call handles both "connect to a saved
/// network" and "connect to a new one" without needing to track which case
/// applies ourselves.
async fn handle_connect(
    conn: &zbus::Connection,
    ssid: &str,
    security: ApSecurity,
    password: Option<&str>,
) -> anyhow::Result<()> {
    let device_path = find_wireless_device(conn).await?;
    let nm_proxy = NetworkManagerProxy::new(conn).await?;

    let mut wireless_settings: HashMap<String, Value<'_>> = HashMap::new();
    wireless_settings.insert("ssid".to_string(), Value::new(ssid.as_bytes().to_vec()));

    let mut connection_settings: HashMap<String, HashMap<String, Value<'_>>> = HashMap::new();
    connection_settings.insert("802-11-wireless".to_string(), wireless_settings);

    if let Some(password) = password {
        connection_settings.insert(
            "802-11-wireless-security".to_string(),
            build_security_settings(security, password),
        );
    }

    let no_specific_object = ObjectPath::try_from("/")?;
    nm_proxy
        .add_and_activate_connection(connection_settings, &device_path, &no_specific_object)
        .await?;

    // report the outcome once NetworkManager finishes trying, so the menu
    // can show something more useful than a connect button that silently
    // does nothing on a wrong password
    let conn_clone = conn.clone();
    let ssid_owned = ssid.to_string();
    relm4::spawn(async move {
        watch_connect_outcome(conn_clone, device_path, ssid_owned).await;
    });

    Ok(())
}

/// Watches a wireless device's `StateChanged` signal until it reaches
/// `Activated` or `Failed` (or a timeout elapses), then broadcasts the
/// outcome as a [`NetworkEvent`].
///
/// A `Failed` transition with reason `NoSecrets` specifically means the
/// password was missing, wrong, or otherwise rejected; anything else is
/// reported as a generic failure.
async fn watch_connect_outcome(conn: zbus::Connection, device_path: OwnedObjectPath, ssid: String) {
    const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);

    let builder = match NetworkDeviceProxy::builder(&conn).path(&device_path) {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!("invalid device path {device_path} while watching connect outcome: {e}");
            return;
        }
    };
    let device_proxy = match builder.build().await {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!("couldn't build device proxy to watch connect outcome: {e}");
            return;
        }
    };

    let mut stream = match device_proxy.receive_device_state_changed().await {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("couldn't subscribe to device state changes for {ssid}: {e}");
            return;
        }
    };

    let outcome = tokio::time::timeout(CONNECT_TIMEOUT, async {
        while let Some(signal) = stream.next().await {
            let Ok(args) = signal.args() else {
                continue;
            };

            match DeviceState::from(args.new_state) {
                DeviceState::Activated => {
                    return Some(NetworkEvent::ConnectionSucceeded { ssid: ssid.clone() });
                }
                DeviceState::Failed => {
                    let reason =
                        if DeviceStateReason::from(args.reason) == DeviceStateReason::NoSecrets {
                            ConnectFailureReason::WrongPassword
                        } else {
                            ConnectFailureReason::Other
                        };
                    return Some(NetworkEvent::ConnectionFailed {
                        ssid: ssid.clone(),
                        reason,
                    });
                }
                // still working through intermediate states (Prepare,
                // Config, NeedAuth, IpConfig, ...)
                _ => continue,
            }
        }
        None
    })
    .await;

    let event = match outcome {
        Ok(Some(event)) => event,
        Ok(None) => {
            tracing::debug!("connect outcome stream for {ssid} ended without a definitive result");
            return;
        }
        Err(_) => {
            tracing::debug!("timed out waiting for a connect outcome for {ssid}");
            NetworkEvent::ConnectionFailed {
                ssid,
                reason: ConnectFailureReason::Other,
            }
        }
    };

    let _ = events::event_tx().send(event);
}

/// Builds the `802-11-wireless-security` settings dict for a password-based
/// connection attempt. Returns an empty map for security schemes that don't
/// take an inline password (`Open`, `Enterprise`); callers are expected to
/// have already excluded those via [`ApSecurity::requires_password`].
fn build_security_settings(security: ApSecurity, password: &str) -> HashMap<String, Value<'_>> {
    let mut settings = HashMap::new();

    match security {
        ApSecurity::Psk => {
            settings.insert("key-mgmt".to_string(), Value::new("wpa-psk"));
            settings.insert("psk".to_string(), Value::new(password.to_string()));
        }
        ApSecurity::Sae => {
            settings.insert("key-mgmt".to_string(), Value::new("sae"));
            settings.insert("psk".to_string(), Value::new(password.to_string()));
        }
        ApSecurity::Wep => {
            // NM_WEP_KEY_TYPE_KEY; wep-key0 holds the passphrase/hex key
            settings.insert("wep-key-type".to_string(), Value::new(1u32));
            settings.insert("wep-key0".to_string(), Value::new(password.to_string()));
        }
        ApSecurity::Open | ApSecurity::Enterprise => {}
    }

    settings
}

async fn handle_disconnect(conn: &zbus::Connection) -> anyhow::Result<()> {
    let nm_proxy = NetworkManagerProxy::new(conn).await?;
    let primary_path = nm_proxy.primary_connection().await?;

    if primary_path.as_str() == "/" {
        anyhow::bail!("no active connection to disconnect");
    }

    nm_proxy.deactivate_connection(&primary_path).await?;
    Ok(())
}

async fn handle_forget(
    conn: &zbus::Connection,
    connection_path: &OwnedObjectPath,
) -> anyhow::Result<()> {
    let connection_proxy = SettingsConnectionProxy::builder(conn)
        .path(connection_path)?
        .build()
        .await?;
    connection_proxy.delete().await?;
    Ok(())
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

    // resolve whichever connection is primary or, if none is yet, being
    // activated - so device_kind is known as soon as NetworkManager picks a
    // device, well before is_connected below can be true
    let connection_path = if primary_connection_path.as_str() != "/" {
        Some(primary_connection_path.clone())
    } else {
        nm_proxy
            .activating_connection()
            .await
            .ok()
            .filter(|path| path.as_str() != "/")
    };
    let resolved_device = match connection_path {
        Some(path) => resolve_active_device(conn, &path)
            .await
            .inspect_err(|e| tracing::debug!("couldn't resolve active device: {e}"))
            .ok()
            .flatten(),
        None => None,
    };
    let device_kind = resolved_device
        .as_ref()
        .and_then(|(_, device_type)| DeviceKind::from_device_type(*device_type));

    let is_connected = matches!(
        connection_state,
        State::ConnectedLocal | State::ConnectedSite | State::ConnectedGlobal
    );

    let (specific_info, ap_path, wifi_device_path) = match (is_connected, resolved_device) {
        (true, Some((device_path, device_type))) => {
            fetch_specific_info(conn, device_path, device_type)
                .await
                .unwrap_or_else(|e| {
                    tracing::debug!("couldn't fetch device-specific network info: {e}");
                    (None, None, None)
                })
        }
        _ => (None, None, None),
    };

    Ok((
        NetworkInfo {
            connection_state,
            connectivity,
            specific_info,
            wifi_enabled,
            device_kind,
        },
        ap_path,
        wifi_device_path,
    ))
}

/// Resolves the network device backing an active connection and its type.
///
/// Returns `None` if the connection has no associated device yet (e.g. it's
/// still being set up, or is a VPN with no hardware device of its own).
async fn resolve_active_device(
    conn: &zbus::Connection,
    connection_path: &OwnedObjectPath,
) -> anyhow::Result<Option<(OwnedObjectPath, DeviceType)>> {
    let active_conn_proxy = ActiveConnectionProxy::builder(conn)
        .path(connection_path)?
        .build()
        .await?;

    let active_device_paths = active_conn_proxy.devices().await?;
    tracing::debug!("active network device paths: {:?}", active_device_paths);

    let Some(device_path) = active_device_paths.first() else {
        return Ok(None);
    };

    let device_proxy = NetworkDeviceProxy::builder(conn)
        .path(device_path)?
        .build()
        .await?;

    let device_type = device_proxy.device_type().await?;

    Ok(Some((device_path.clone(), device_type)))
}

/// Builds the connected device's type-specific info (wired or wifi) for an
/// already-resolved device.
///
/// Returns `Ok((None, None, None))` for a device type we don't render
/// distinctly (e.g. mobile broadband). The third element is the wireless
/// device's own object path, present whenever the connected device is wifi
/// regardless of whether an access point could be read.
async fn fetch_specific_info(
    conn: &zbus::Connection,
    device_path: OwnedObjectPath,
    device_type: DeviceType,
) -> anyhow::Result<(
    Option<SpecificNetworkInfo>,
    Option<OwnedObjectPath>,
    Option<OwnedObjectPath>,
)> {
    match device_type {
        DeviceType::Ethernet => Ok((Some(SpecificNetworkInfo::Wired), None, None)),
        DeviceType::Wifi => {
            // always report the wifi device path so the caller can subscribe
            // to ActiveAccessPoint changes even if there's no access point to
            // read yet (e.g. mid-association); otherwise we'd never notice
            // once one becomes available until an unrelated event fires
            match get_wifi_info(conn, &device_path).await {
                Ok((ssid, strength, ap_path)) => Ok((
                    Some(SpecificNetworkInfo::WiFi {
                        wifi_ssid: ssid,
                        wifi_strength: strength,
                    }),
                    Some(ap_path),
                    Some(device_path),
                )),
                Err(e) => {
                    tracing::debug!("couldn't fetch wifi access point info: {e}");
                    Ok((None, None, Some(device_path)))
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
