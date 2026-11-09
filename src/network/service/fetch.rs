//! Reading the current network state from NetworkManager.

use zbus::zvariant::OwnedObjectPath;

use crate::network::{
    dbus::{
        AccessPointProxy, ActiveConnectionProxy, NetworkDeviceProxy, NetworkManagerProxy,
        WirelessDeviceProxy, proxy_at,
    },
    state::{DeviceKind, NetworkInfo, SpecificNetworkInfo, WifiConnection},
    types::{NM_DEVICE_TYPE_ETHERNET, NM_DEVICE_TYPE_WIFI, State},
};

/// Finds the first wifi-capable network device, regardless of its current
/// connection state (unlike the wireless device path tracked in
/// `SubscriptionTasks`, which is only populated once a wifi connection is
/// already active).
pub(super) async fn find_wireless_device(
    conn: &zbus::Connection,
) -> anyhow::Result<OwnedObjectPath> {
    let nm_proxy = NetworkManagerProxy::new(conn).await?;
    let device_paths = nm_proxy.get_devices().await?;

    for path in device_paths {
        let Ok(device_proxy) = proxy_at::<NetworkDeviceProxy>(conn, &path).await else {
            continue;
        };
        if let Ok(NM_DEVICE_TYPE_WIFI) = device_proxy.device_type().await {
            return Ok(path);
        }
    }

    anyhow::bail!("no wireless device found")
}

/// Fetches full network info for the given primary connection path.
///
/// Returns the `NetworkInfo`, the wifi device's active access point object
/// path if it's associated (for setting up a strength subscription), and the
/// wireless device's object path if there is one (for setting up an
/// active-access-point subscription).
///
/// Only fails if the overall connection state and connectivity themselves
/// can't be read (i.e. NetworkManager itself is unreachable). If the
/// device-specific details (which device, which access point) can't be
/// determined - e.g. because `ActiveAccessPoint` hasn't been populated yet
/// mid-association - `specific_info` degrades to `None` rather than
/// discarding the whole fetch, so [`NETWORK_STATE`] is never left stale
/// after a transient read failure.
pub(super) async fn fetch_network_info(
    conn: &zbus::Connection,
    nm_proxy: &NetworkManagerProxy<'_>,
    primary_connection_path: OwnedObjectPath,
) -> anyhow::Result<(
    NetworkInfo,
    Option<OwnedObjectPath>,
    Option<OwnedObjectPath>,
)> {
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

    // the wifi device is read on its own so the menu still sees the wifi
    // network while ethernet or a vpn is primary; prefer the device behind
    // the primary connection on machines with several wifi cards
    let wifi_device_path = match &resolved_device {
        Some((path, NM_DEVICE_TYPE_WIFI)) => Some(path.clone()),
        _ => find_wireless_device(conn).await.ok(),
    };
    let wifi = match &wifi_device_path {
        Some(path) => get_wifi_info(conn, path)
            .await
            .inspect_err(|e| tracing::debug!("couldn't fetch wifi access point info: {e}"))
            .ok(),
        None => None,
    };

    let specific_info = match (is_connected, &resolved_device) {
        (true, Some((_, NM_DEVICE_TYPE_ETHERNET))) => Some(SpecificNetworkInfo::Wired),
        (true, Some((_, NM_DEVICE_TYPE_WIFI))) => {
            wifi.as_ref()
                .map(|(ssid, strength, _)| SpecificNetworkInfo::WiFi {
                    wifi_ssid: ssid.clone(),
                    wifi_strength: *strength,
                })
        }
        _ => None,
    };
    let ap_path = wifi.as_ref().map(|(_, _, ap_path)| ap_path.clone());
    let wifi = wifi.map(|(ssid, strength, _)| WifiConnection { ssid, strength });

    Ok((
        NetworkInfo {
            connection_state,
            connectivity,
            specific_info,
            wifi_enabled,
            device_kind,
            wifi,
        },
        ap_path,
        wifi_device_path,
    ))
}

/// Resolves the network device backing an active connection and its type.
///
/// Returns `None` if the connection has no associated device yet (e.g. it's
/// still being set up, or is a VPN with no hardware device of its own).
pub(super) async fn resolve_active_device(
    conn: &zbus::Connection,
    connection_path: &OwnedObjectPath,
) -> anyhow::Result<Option<(OwnedObjectPath, u32)>> {
    let active_conn_proxy = proxy_at::<ActiveConnectionProxy>(conn, connection_path).await?;

    let active_device_paths = active_conn_proxy.devices().await?;
    tracing::debug!("active network device paths: {:?}", active_device_paths);

    let Some(device_path) = active_device_paths.first() else {
        return Ok(None);
    };

    let device_proxy = proxy_at::<NetworkDeviceProxy>(conn, device_path).await?;

    let device_type = device_proxy.device_type().await?;

    Ok(Some((device_path.clone(), device_type)))
}

/// Returns the SSID, current strength, and object path of the active access
/// point for the given wireless device.
pub(super) async fn get_wifi_info(
    conn: &zbus::Connection,
    device_path: &zbus::zvariant::OwnedObjectPath,
) -> anyhow::Result<(String, u8, OwnedObjectPath)> {
    let wifi_proxy = proxy_at::<WirelessDeviceProxy>(conn, device_path).await?;

    let ap_path = wifi_proxy.active_access_point().await?;

    // check if access point path is valid (not "/" which means no connection)
    if ap_path.as_str() == "/" {
        anyhow::bail!("no active access point");
    }

    let ap_proxy = proxy_at::<AccessPointProxy>(conn, &ap_path).await?;

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
