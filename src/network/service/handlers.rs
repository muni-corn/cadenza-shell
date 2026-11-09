//! Handlers for commands sent by the network menu.

use std::collections::HashMap;

use futures_lite::StreamExt;
use tokio::sync::mpsc::UnboundedSender;
use zbus::zvariant::{ObjectPath, OwnedObjectPath, Value};

use super::{
    NetworkPropertyChange, connect_outcome::watch_connect_outcome, fetch::find_wireless_device,
};
use crate::network::{
    commands::NetworkCommand,
    dbus::{NetworkDeviceProxy, NetworkManagerProxy, SettingsConnectionProxy, WirelessDeviceProxy},
    events::{self, ConnectFailureReason, NetworkEvent},
    password::Password,
    scan,
    security::ApSecurity,
    state::{NETWORK_STATE, WIFI_SCAN_STATE},
};

/// Dispatches a single UI-issued command to its D-Bus handler, logging on
/// failure. Failures are not fatal to the service; the user can simply
/// retry from the menu.
pub(super) async fn handle_command(conn: &zbus::Connection, cmd: NetworkCommand) {
    match cmd {
        NetworkCommand::SetWifiEnabled(enabled) => {
            if let Err(e) = handle_set_wifi_enabled(conn, enabled).await {
                tracing::error!("couldn't set wifi enabled to {enabled}: {e}");

                // nothing changed on the bus, so republish the real value to
                // move the menu's switch back
                let actual = match NetworkManagerProxy::new(conn).await {
                    Ok(nm_proxy) => nm_proxy.wireless_enabled().await.ok(),
                    Err(_) => None,
                };
                let mut state = NETWORK_STATE.write();
                state.wifi_enabled = actual.unwrap_or(state.wifi_enabled);
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
            saved_connections,
        } => {
            if let Err(e) = handle_connect(
                conn,
                &ssid,
                security,
                password.as_ref().map(Password::expose),
                saved_connections,
            )
            .await
            {
                tracing::error!("couldn't connect to {ssid}: {e}");

                // the menu has already closed its password prompt, so it
                // needs to hear about failures that happen before activation
                let _ = events::event_tx().send(NetworkEvent::ConnectionFailed {
                    ssid,
                    reason: ConnectFailureReason::Other,
                });
            }
        }
        NetworkCommand::Disconnect => {
            if let Err(e) = handle_disconnect(conn).await {
                tracing::error!("couldn't disconnect: {e}");
            }
        }
        NetworkCommand::Forget(connection_paths) => {
            for connection_path in connection_paths {
                if let Err(e) = handle_forget(conn, &connection_path).await {
                    tracing::error!("couldn't forget connection {connection_path}: {e}");
                }
            }
        }
    }
}

pub(super) async fn handle_set_wifi_enabled(
    conn: &zbus::Connection,
    enabled: bool,
) -> anyhow::Result<()> {
    let nm_proxy = NetworkManagerProxy::new(conn).await?;
    nm_proxy.set_wireless_enabled(enabled).await?;
    Ok(())
}

/// Requests a scan, then refetches and publishes the access point list.
///
/// `RequestScan` only asks NetworkManager to *start* scanning; it doesn't
/// wait for completion, so the immediate refetch below shows the previous
/// results. Fresh results are published once `LastScan` advances (see
/// [`ensure_scan_watcher`]).
pub(super) async fn handle_scan(conn: &zbus::Connection) -> anyhow::Result<()> {
    // NetworkManager rejects scans while the radio is off
    if !NETWORK_STATE.read().wifi_enabled {
        tracing::debug!("wifi is disabled, skipping scan");
        return Ok(());
    }

    WIFI_SCAN_STATE.write().scanning = true;
    let result = scan_and_refresh(conn).await;

    // reset even on early errors so the menu doesn't show "Scanning..." forever
    WIFI_SCAN_STATE.write().scanning = false;

    result
}

/// Requests a scan from the wireless device and refreshes the access point
/// list.
pub(super) async fn scan_and_refresh(conn: &zbus::Connection) -> anyhow::Result<()> {
    let device_path = find_wireless_device(conn).await?;
    let wifi_proxy = WirelessDeviceProxy::builder(conn)
        .path(&device_path)?
        .build()
        .await?;

    // the caller logs a failed request, so only the refresh is logged here
    let scan_result = wifi_proxy.request_scan(HashMap::new()).await;

    if let Err(e) = refresh_access_points(conn, &device_path).await {
        tracing::warn!("couldn't refresh access points after scan request: {e}");
    }

    scan_result.map_err(anyhow::Error::from)
}

/// Refetches the access point list for the given wireless device and
/// publishes it to [`WIFI_SCAN_STATE`].
pub(super) async fn refresh_access_points(
    conn: &zbus::Connection,
    device_path: &OwnedObjectPath,
) -> anyhow::Result<()> {
    let access_points = scan::fetch_access_points(conn, device_path)
        .await
        .inspect_err(|e| tracing::warn!("couldn't fetch access points: {e}"))
        .unwrap_or_default();

    WIFI_SCAN_STATE.write().access_points = access_points;

    Ok(())
}

/// Refetches the access point list for the wifi device, if there is one.
pub(super) async fn refresh_wifi_access_points(conn: &zbus::Connection) {
    let result = match find_wireless_device(conn).await {
        Ok(device_path) => refresh_access_points(conn, &device_path).await,
        Err(e) => Err(e),
    };
    if let Err(e) = result {
        tracing::debug!("couldn't refresh access points: {e}");
    }
}

/// Starts watching the wifi device's `LastScan` property, unless a watcher
/// for the same device is already running, so each completed scan
/// publishes fresh results.
pub(super) async fn ensure_scan_watcher(
    conn: &zbus::Connection,
    event_tx: &UnboundedSender<NetworkPropertyChange>,
    watcher: &mut Option<(OwnedObjectPath, tokio::task::AbortHandle)>,
) {
    let Ok(device_path) = find_wireless_device(conn).await else {
        return;
    };
    if let Some((watched_path, handle)) = watcher
        && *watched_path == device_path
        && !handle.is_finished()
    {
        return;
    }
    if let Some((_, handle)) = watcher.take() {
        handle.abort();
    }

    let conn = conn.clone();
    let tx = event_tx.clone();
    let path = device_path.clone();
    let handle = relm4::spawn(async move {
        let proxy = match WirelessDeviceProxy::builder(&conn).path(&path) {
            Ok(builder) => builder.build().await,
            Err(e) => Err(e),
        };
        let proxy = match proxy {
            Ok(proxy) => proxy,
            Err(e) => {
                tracing::warn!("couldn't watch wifi scans on {path}: {e}");
                return;
            }
        };

        let mut changes = proxy.receive_last_scan_changed().await;
        // the first item is the current value, not a completed scan
        changes.next().await;
        while changes.next().await.is_some() {
            if tx.send(NetworkPropertyChange::ScanCompleted).is_err() {
                break;
            }
        }
    })
    .abort_handle();

    *watcher = Some((device_path, handle));
}

/// Connects to a wifi network by SSID.
///
/// A saved profile is activated directly when no password is given;
/// `AddAndActivateConnection` always creates a new profile, so using it for
/// saved networks would pile up duplicates without their secrets. With a
/// password, a new profile is created from a partial settings dict that
/// NetworkManager completes, and any saved profile for the network is
/// deleted first because its secrets were rejected.
pub(super) async fn handle_connect(
    conn: &zbus::Connection,
    ssid: &str,
    security: ApSecurity,
    password: Option<&str>,
    saved_connections: Vec<OwnedObjectPath>,
) -> anyhow::Result<()> {
    let device_path = find_wireless_device(conn).await?;
    let nm_proxy = NetworkManagerProxy::new(conn).await?;
    let no_specific_object = ObjectPath::try_from("/")?;

    if let (Some(saved), None) = (saved_connections.first(), password) {
        let active_connection = nm_proxy
            .activate_connection(saved, &device_path, &no_specific_object)
            .await?;
        spawn_outcome_watcher(conn, active_connection, ssid, None);
        return Ok(());
    }

    for saved in saved_connections {
        if let Err(e) = handle_forget(conn, &saved).await {
            tracing::warn!("couldn't delete the rejected profile for {ssid}: {e}");
        }
    }

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

    let (profile, active_connection) = nm_proxy
        .add_and_activate_connection(connection_settings, &device_path, &no_specific_object)
        .await?;
    spawn_outcome_watcher(conn, active_connection, ssid, Some(profile));

    Ok(())
}

/// Reports the outcome of a connection attempt once NetworkManager finishes
/// trying, so the menu can show something more useful than a connect button
/// that silently does nothing on a wrong password.
///
/// `created_profile` is deleted if the attempt fails, so wrong passwords
/// don't leave profiles behind.
pub(super) fn spawn_outcome_watcher(
    conn: &zbus::Connection,
    active_connection: OwnedObjectPath,
    ssid: &str,
    created_profile: Option<OwnedObjectPath>,
) {
    let conn = conn.clone();
    let ssid = ssid.to_string();
    relm4::spawn(async move {
        let outcome = watch_connect_outcome(&conn, &active_connection, &ssid)
            .await
            .unwrap_or_else(|e| {
                tracing::warn!("couldn't watch the connection attempt for {ssid}: {e}");
                None
            });

        let failed = matches!(outcome, Some(NetworkEvent::ConnectionFailed { .. }));
        if let Some(event) = outcome {
            let _ = events::event_tx().send(event);
        }

        if failed
            && let Some(profile) = created_profile
            && let Err(e) = handle_forget(&conn, &profile).await
        {
            tracing::warn!("couldn't delete the failed profile for {ssid}: {e}");
        }
    });
}

/// Builds the `802-11-wireless-security` settings dict for a password-based
/// connection attempt. Returns an empty map for security schemes that don't
/// take an inline password (`Open`, `Enterprise`); callers are expected to
/// have already excluded those via [`ApSecurity::requires_password`].
pub(super) fn build_security_settings(
    security: ApSecurity,
    password: &str,
) -> HashMap<String, Value<'_>> {
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
            settings.insert(
                "wep-key-type".to_string(),
                Value::new(wep_key_type(password)),
            );
            settings.insert("wep-key0".to_string(), Value::new(password.to_string()));
        }
        ApSecurity::Open | ApSecurity::Enterprise => {}
    }

    settings
}

/// Picks the `NMWepKeyType` for a WEP password: raw keys are 5 or 13 ASCII
/// characters or 10 or 26 hex digits (`KEY`, 1); anything else is hashed as
/// a passphrase (`PASSPHRASE`, 2).
pub(super) fn wep_key_type(password: &str) -> u32 {
    let is_hex = password.chars().all(|c| c.is_ascii_hexdigit());
    match password.len() {
        5 | 13 => 1,
        10 | 26 if is_hex => 1,
        _ => 2,
    }
}

/// Disconnects the wifi device, leaving any wired or vpn connection alone.
pub(super) async fn handle_disconnect(conn: &zbus::Connection) -> anyhow::Result<()> {
    let device_path = find_wireless_device(conn).await?;
    NetworkDeviceProxy::builder(conn)
        .path(&device_path)?
        .build()
        .await?
        .disconnect()
        .await?;
    Ok(())
}

pub(super) async fn handle_forget(
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_wep_keys_are_detected() {
        assert_eq!(wep_key_type("abcde"), 1);
        assert_eq!(wep_key_type("0123456789"), 1);
        assert_eq!(wep_key_type(&"a1".repeat(13)), 1);
    }

    #[test]
    fn other_wep_passwords_are_passphrases() {
        assert_eq!(wep_key_type("a longer passphrase"), 2);
        assert_eq!(wep_key_type("not hex at"), 2);
    }
}
