mod connect_outcome;
mod fetch;
mod handlers;
mod watchers;

use std::time::Duration;

use tokio::{
    sync::mpsc::{self, UnboundedReceiver, UnboundedSender},
    task::AbortHandle,
    time::MissedTickBehavior,
};
use zbus::zvariant::OwnedObjectPath;

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
        types::{ConnectivityState, State},
    },
    settings, sleep_monitor,
    utils::shared_state::set_if_changed,
};

/// Events produced by the watcher tasks for the service loop.
#[derive(Clone, Debug)]
pub enum NetworkPropertyChange {
    State(State),
    Connectivity(ConnectivityState),
    /// The active access point's signal strength changed.
    Strength(u8),
    /// The wifi radio was turned on or off.
    WifiEnabled(bool),
    /// Something changed that needs a full refetch, such as the primary
    /// connection or the active access point (roaming to another BSSID
    /// doesn't change `PrimaryConnection`). The reason is only logged.
    Refetch(&'static str),
    /// NetworkManager (re)appeared on the bus under a new unique name (e.g.
    /// `systemctl restart NetworkManager`, or it starting after us). The
    /// existing proxies filled their property caches from the previous
    /// owner and can't refill them, so their property streams stop
    /// producing values; the watchers are rebuilt against the new owner.
    NetworkManagerRestarted,
    /// NetworkManager left the bus without a replacement.
    NetworkManagerVanished,
    /// The wifi device finished a scan (its `LastScan` advanced).
    ScanCompleted,
}

/// The per-connection subscriptions, each with the object path it watches,
/// so they're only respawned when that path changes.
#[derive(Default)]
struct SubscriptionTasks {
    strength: Option<(OwnedObjectPath, AbortHandle)>,
    active_ap: Option<(OwnedObjectPath, AbortHandle)>,
}

impl SubscriptionTasks {
    fn abort_all(&mut self) {
        for (_, handle) in [self.strength.take(), self.active_ap.take()]
            .into_iter()
            .flatten()
        {
            handle.abort();
        }
    }
}

/// Keeps `slot` subscribed to `path`, respawning the task only when the
/// path changed or the previous task ended.
fn resubscribe(
    slot: &mut Option<(OwnedObjectPath, AbortHandle)>,
    path: Option<OwnedObjectPath>,
    spawn: impl FnOnce(OwnedObjectPath) -> AbortHandle,
) {
    let unchanged = matches!((&*slot, &path), (Some((current, handle)), Some(path))
        if current == path && !handle.is_finished());
    if unchanged {
        return;
    }

    if let Some((_, handle)) = slot.take() {
        handle.abort();
    }
    *slot = path.map(|path| (path.clone(), spawn(path)));
}

/// Runs the network service: watches NetworkManager, publishes
/// [`NETWORK_STATE`] and [`WIFI_SCAN_STATE`], and handles commands from the
/// menu, all from a single loop.
pub async fn run_network_service() {
    // opened first, so a second instance bails out before starting tasks
    let Some(mut cmd_rx) = commands::COMMANDS.open() else {
        tracing::warn!("network service started more than once; extra instance exiting");
        return;
    };

    let (mut service, mut event_rx) = match NetworkService::start().await {
        Ok(started) => started,
        Err(e) => {
            tracing::error!("failed to setup network property watching: {e}");
            return;
        }
    };

    let mut wake_rx = sleep_monitor::subscribe_wake();

    // periodically reconcile full state so a missed or dropped D-Bus signal
    // never leaves the tile stale indefinitely
    let mut reconcile = tokio::time::interval(Duration::from_secs(
        settings::get_config().network.reconcile_interval,
    ));
    reconcile.set_missed_tick_behavior(MissedTickBehavior::Skip);
    // the first tick fires immediately; the initial fetch happens below
    reconcile.tick().await;

    service.refetch("startup").await;

    loop {
        tokio::select! {
            Some(event) = event_rx.recv() => service.handle_event(event).await,
            Some(cmd) = cmd_rx.recv() => service.handle_command(cmd).await,
            Ok(()) = wake_rx.recv() => service.refetch("system wake").await,
            _ = reconcile.tick() => {
                // reconciling while NetworkManager is gone would only fail
                if service.nm_present {
                    service.refetch("periodic reconcile").await;
                }
            }
            else => break,
        }
    }
    tracing::warn!("network service has stopped receiving events");
}

/// Everything the network service loop owns. Dropping it stops every task
/// it started.
struct NetworkService {
    conn: zbus::Connection,
    /// Kept for the service's lifetime so its property cache stays warm;
    /// rebuilt when NetworkManager restarts.
    nm: NetworkManagerProxy<'static>,
    event_tx: UnboundedSender<NetworkPropertyChange>,
    /// Watches NetworkManager's own properties; respawned when it restarts.
    property_watcher: AbortHandle,
    name_owner_task: Option<AbortHandle>,
    subscriptions: SubscriptionTasks,
    scan_watcher: Option<(OwnedObjectPath, AbortHandle)>,
    /// Whether NetworkManager currently owns its bus name.
    nm_present: bool,
}

impl NetworkService {
    /// Connects to the system bus and starts the property watchers.
    async fn start() -> anyhow::Result<(Self, UnboundedReceiver<NetworkPropertyChange>)> {
        let conn = zbus::Connection::system().await?;
        let (event_tx, event_rx) = mpsc::unbounded_channel();

        let nm = NetworkManagerProxy::new(&conn).await?;
        let property_watcher = spawn_property_watchers(&conn, &event_tx).await?;
        let name_owner_task = spawn_name_owner_watcher(&conn, &event_tx).await;

        let service = Self {
            conn,
            nm,
            event_tx,
            property_watcher,
            name_owner_task,
            subscriptions: SubscriptionTasks::default(),
            scan_watcher: None,
            nm_present: true,
        };
        Ok((service, event_rx))
    }

    async fn handle_event(&mut self, event: NetworkPropertyChange) {
        match event {
            NetworkPropertyChange::State(state) => {
                update_network_state(|info| info.connection_state = state);

                // refetch as soon as a connection starts activating, so
                // device_kind (and thus the wifi/wired acquiring icon) is
                // known right away - ActivatingConnection has no dedicated
                // watcher of its own, unlike PrimaryConnection
                if state == State::Connecting {
                    self.refetch("connection activating").await;
                }

                // if we just transitioned to a connected state but
                // specific_info is still None (e.g. the wake refetch ran before
                // NM finished reconnecting and no PrimaryConnection change will
                // fire since we reconnected to the same network), do a full
                // refetch now to populate the missing device details
                let missing_details = NETWORK_STATE.read().specific_info.is_none();
                if let State::ConnectedLocal | State::ConnectedSite | State::ConnectedGlobal = state
                    && missing_details
                {
                    self.refetch("connected without device details").await;
                }
            }
            NetworkPropertyChange::Connectivity(connectivity) => {
                update_network_state(|info| info.connectivity = connectivity);
            }
            NetworkPropertyChange::Strength(strength) => update_network_state(|info| {
                if let Some(wifi) = &mut info.wifi {
                    wifi.strength = strength;
                }
                if let Some(SpecificNetworkInfo::WiFi {
                    ref mut wifi_strength,
                    ..
                }) = info.specific_info
                {
                    *wifi_strength = strength;
                }
            }),
            NetworkPropertyChange::WifiEnabled(enabled) => {
                update_network_state(|info| info.wifi_enabled = enabled);
            }
            NetworkPropertyChange::Refetch(reason) => self.refetch(reason).await,
            NetworkPropertyChange::NetworkManagerVanished => {
                tracing::warn!("networkmanager disappeared from the bus");
                self.nm_present = false;
                self.subscriptions.abort_all();
                set_if_changed(&NETWORK_STATE, NetworkInfo::default());
                set_if_changed(&WIFI_SCAN_STATE, Default::default());
            }
            NetworkPropertyChange::NetworkManagerRestarted => {
                tracing::info!(
                    "networkmanager reappeared on the bus, re-establishing subscriptions"
                );
                self.nm_present = true;
                self.property_watcher.abort();

                // a fresh proxy refills its property cache from the new owner
                match NetworkManagerProxy::new(&self.conn).await {
                    Ok(nm) => self.nm = nm,
                    Err(e) => tracing::error!("couldn't rebuild the networkmanager proxy: {e}"),
                }
                match spawn_property_watchers(&self.conn, &self.event_tx).await {
                    Ok(watcher) => self.property_watcher = watcher,
                    Err(e) => tracing::error!("couldn't re-establish property watchers: {e}"),
                }
                self.refetch("networkmanager restarted").await;
            }
            NetworkPropertyChange::ScanCompleted => {
                tracing::debug!("wifi scan completed, refreshing access points");
                refresh_wifi_access_points(&self.conn).await;
            }
        }
    }

    async fn handle_command(&mut self, cmd: NetworkCommand) {
        // results only arrive once LastScan advances, so make sure something
        // is listening before a scan is requested
        if matches!(cmd, NetworkCommand::Scan) {
            ensure_scan_watcher(&self.conn, &self.event_tx, &mut self.scan_watcher).await;
        }

        let changes_saved_networks = matches!(
            cmd,
            NetworkCommand::Connect { .. } | NetworkCommand::Disconnect | NetworkCommand::Forget(_)
        );
        handle_command(&self.conn, cmd).await;

        // saved and connected markers in the menu changed
        if changes_saved_networks {
            refresh_wifi_access_points(&self.conn).await;
        }
    }

    /// Fetches current NM state, updates [`NETWORK_STATE`], and
    /// (re)subscribes to access point signal strength changes and
    /// active-access-point changes if on WiFi.
    ///
    /// Subscriptions are only replaced when the access point or device they
    /// watch changed, and a failed fetch leaves them running.
    async fn refetch(&mut self, reason: &str) {
        tracing::debug!("refetching network state: {reason}");
        if let Err(e) = self.try_refetch().await {
            tracing::warn!("couldn't refetch network state ({reason}): {e}");
        }
    }

    async fn try_refetch(&mut self) -> anyhow::Result<()> {
        let primary_path = self.nm.primary_connection().await?;
        let (info, ap_path, wifi_device_path) =
            fetch_network_info(&self.conn, &self.nm, primary_path).await?;

        tracing::debug!("fetched network info: {:?}", info);
        set_if_changed(&NETWORK_STATE, info);

        // follow strength changes on the current access point
        let (conn, tx) = (self.conn.clone(), self.event_tx.clone());
        resubscribe(&mut self.subscriptions.strength, ap_path.clone(), |path| {
            relm4::spawn(subscribe_ap_strength(conn, path, tx)).abort_handle()
        });

        // follow active access point changes on the wireless device, so
        // roaming to a different BSSID (which doesn't change PrimaryConnection)
        // still triggers a refetch instead of leaving strength/SSID frozen on
        // the now-disappeared access point object
        let (conn, tx) = (self.conn.clone(), self.event_tx.clone());
        resubscribe(
            &mut self.subscriptions.active_ap,
            wifi_device_path,
            |path| relm4::spawn(subscribe_active_ap(conn, path, ap_path, tx)).abort_handle(),
        );

        Ok(())
    }
}

/// Applies `update` to a copy of the network state and publishes it only if
/// something changed, since every write wakes the tile and the menu.
fn update_network_state(update: impl FnOnce(&mut NetworkInfo)) {
    let mut info = NETWORK_STATE.read().clone();
    update(&mut info);
    set_if_changed(&NETWORK_STATE, info);
}

impl Drop for NetworkService {
    fn drop(&mut self) {
        self.property_watcher.abort();
        self.subscriptions.abort_all();
        if let Some(handle) = &self.name_owner_task {
            handle.abort();
        }
        if let Some((_, handle)) = &self.scan_watcher {
            handle.abort();
        }
    }
}
