//! Subscriptions that turn NetworkManager signals into service events.

use futures_lite::StreamExt;
use tokio::sync::mpsc::UnboundedSender;
use zbus::zvariant::OwnedObjectPath;

use super::{NetworkPropertyChange, PropertyWatcherTasks};
use crate::network::dbus::{AccessPointProxy, NetworkManagerProxy, WirelessDeviceProxy};

/// Spawns the top-level NetworkManager property watchers (state,
/// connectivity, primary connection, wifi enabled), forwarding changes into
/// `event_tx`.
///
/// Callable both at startup and again after a `NetworkManagerRestarted`
/// event, since each proxy's property cache is filled once from whichever
/// process owned `org.freedesktop.NetworkManager` when it was built.
pub(super) async fn spawn_property_watchers(
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
            // the new path itself isn't needed; any change triggers a refetch
            if let Err(e) = change.get().await {
                tracing::error!("couldn't get primary connection change value: {e}");
                continue;
            }
            event_tx_clone
                .send(NetworkPropertyChange::Refetch("primary connection changed"))
                .unwrap_or_else(|e| {
                    tracing::error!("couldn't send primary connection change: {e}")
                });
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
pub(super) async fn spawn_name_owner_watcher(
    conn: &zbus::Connection,
    event_tx: &UnboundedSender<NetworkPropertyChange>,
) -> Option<tokio::task::AbortHandle> {
    let dbus_proxy = match zbus::fdo::DBusProxy::new(conn).await {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("couldn't create DBus proxy for name owner watching: {e}");
            return None;
        }
    };

    let mut stream = match dbus_proxy.receive_name_owner_changed().await {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("couldn't subscribe to NameOwnerChanged: {e}");
            return None;
        }
    };

    let event_tx = event_tx.clone();
    let handle = relm4::spawn(async move {
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
            // new unique name); either way the property caches need to be
            // refilled from the current owner
            if args.new_owner.is_some() {
                event_tx
                    .send(NetworkPropertyChange::NetworkManagerRestarted)
                    .unwrap_or_else(|e| {
                        tracing::error!("couldn't send networkmanager restart event: {e}")
                    });
            } else {
                event_tx
                    .send(NetworkPropertyChange::NetworkManagerVanished)
                    .unwrap_or_else(|e| {
                        tracing::error!("couldn't send networkmanager vanished event: {e}")
                    });
            }
        }
        tracing::warn!("stream for NameOwnerChanged has closed");
    });
    Some(handle.abort_handle())
}

/// Subscribes to strength property changes on an access point and forwards
/// them as [`NetworkPropertyChange::Strength`] events.
///
/// This task runs until the stream closes or the task is aborted (e.g. when
/// the primary connection changes or the system sleeps).
pub(super) async fn subscribe_ap_strength(
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

    // the stream starts with the current value, which may already differ
    // from the fetch; repeats are dropped when the state is written
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
/// forwards them as [`NetworkPropertyChange::Refetch`] events.
///
/// This catches roaming to a different BSSID on the same SSID, which does
/// not change `PrimaryConnection` and would otherwise leave the strength
/// subscription pointed at an access point object that has left the bus.
///
/// This task runs until the stream closes or the task is aborted (e.g. when
/// the primary connection changes or the system sleeps).
pub(super) async fn subscribe_active_ap(
    conn: zbus::Connection,
    device_path: OwnedObjectPath,
    known_ap: Option<OwnedObjectPath>,
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

    // the stream starts with the current value; it only matters if the
    // access point changed between the fetch and this subscription
    if let Some(first) = stream.next().await {
        let current = first.get().await.ok().filter(|path| path.as_str() != "/");
        if current != known_ap {
            tx.send(NetworkPropertyChange::Refetch(
                "active access point changed while subscribing",
            ))
            .unwrap_or_else(|e| tracing::error!("couldn't send active AP change: {e}"));
        }
    }

    while stream.next().await.is_some() {
        tx.send(NetworkPropertyChange::Refetch(
            "active access point changed",
        ))
        .unwrap_or_else(|e| tracing::error!("couldn't send active AP change: {e}"));
    }

    tracing::debug!("active access point subscription for device {device_path} ended");
}
