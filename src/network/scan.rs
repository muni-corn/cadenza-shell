// temporary until run_network_service calls fetch_access_points
#![allow(dead_code)]

use std::collections::HashMap;

use zbus::zvariant::OwnedObjectPath;

use crate::network::{
    dbus::{AccessPointProxy, SettingsConnectionProxy, SettingsProxy, WirelessDeviceProxy},
    state::AccessPointSummary,
    types::ApSecurity,
};

/// A single access point reading, before deduplication by SSID.
#[derive(Debug, Clone, PartialEq)]
struct RawApReading {
    ssid: String,
    strength: u8,
    security: ApSecurity,
}

/// Fetches every access point currently visible to the given wireless
/// device, deduplicates them by SSID (keeping the strongest reading per
/// SSID), and marks which ones already have a saved connection profile.
pub async fn fetch_access_points(
    conn: &zbus::Connection,
    device_path: &OwnedObjectPath,
    active_ssid: Option<&str>,
) -> anyhow::Result<Vec<AccessPointSummary>> {
    let wifi_proxy = WirelessDeviceProxy::builder(conn)
        .path(device_path)?
        .build()
        .await?;
    let ap_paths = wifi_proxy.get_all_access_points().await?;

    let mut readings = Vec::with_capacity(ap_paths.len());
    for ap_path in &ap_paths {
        match read_access_point(conn, ap_path).await {
            Ok(Some(reading)) => readings.push(reading),
            // empty or whitespace-only SSID (e.g. a hidden network); skip
            Ok(None) => {}
            Err(e) => tracing::debug!("couldn't read access point {ap_path}: {e}"),
        }
    }

    let saved = fetch_saved_wifi_connections(conn)
        .await
        .unwrap_or_else(|e| {
            tracing::warn!("couldn't fetch saved wifi connections: {e}");
            HashMap::new()
        });

    Ok(dedupe_and_sort(readings, active_ssid, &saved))
}

/// Reads a single access point's SSID, strength, and security. Returns
/// `Ok(None)` for access points with no usable SSID (hidden networks
/// broadcast an empty or whitespace-only one).
async fn read_access_point(
    conn: &zbus::Connection,
    ap_path: &OwnedObjectPath,
) -> anyhow::Result<Option<RawApReading>> {
    let ap_proxy = AccessPointProxy::builder(conn)
        .path(ap_path)?
        .build()
        .await?;

    let ssid_bytes = ap_proxy.ssid().await?;
    if ssid_bytes.is_empty() {
        return Ok(None);
    }

    let ssid = String::from_utf8_lossy(&ssid_bytes).to_string();
    if ssid.trim().is_empty() {
        return Ok(None);
    }

    let strength = ap_proxy.strength().await?;
    let flags = ap_proxy.flags().await?;
    let wpa_flags = ap_proxy.wpa_flags().await?;
    let rsn_flags = ap_proxy.rsn_flags().await?;
    let security = ApSecurity::from_flags(flags, wpa_flags, rsn_flags);

    Ok(Some(RawApReading {
        ssid,
        strength,
        security,
    }))
}

/// Returns saved wifi connection profiles keyed by SSID.
async fn fetch_saved_wifi_connections(
    conn: &zbus::Connection,
) -> anyhow::Result<HashMap<String, OwnedObjectPath>> {
    let settings_proxy = SettingsProxy::new(conn).await?;
    let connection_paths = settings_proxy.list_connections().await?;

    let mut result = HashMap::with_capacity(connection_paths.len());
    for path in connection_paths {
        let Ok(builder) = SettingsConnectionProxy::builder(conn).path(&path) else {
            continue;
        };
        let Ok(conn_proxy) = builder.build().await else {
            continue;
        };
        let Ok(settings) = conn_proxy.get_settings().await else {
            continue;
        };

        let Some(ssid) = settings
            .get("802-11-wireless")
            .and_then(|wireless| wireless.get("ssid"))
            .and_then(|value| Vec::<u8>::try_from(value.try_clone().ok()?).ok())
        else {
            continue;
        };

        result.insert(String::from_utf8_lossy(&ssid).to_string(), path);
    }

    Ok(result)
}

/// Groups raw per-BSSID readings by SSID (keeping only the strongest signal
/// for each), attaches active/saved status, and sorts with the active
/// connection first, then by descending signal strength.
fn dedupe_and_sort(
    readings: Vec<RawApReading>,
    active_ssid: Option<&str>,
    saved: &HashMap<String, OwnedObjectPath>,
) -> Vec<AccessPointSummary> {
    let mut strongest_by_ssid: HashMap<String, RawApReading> = HashMap::new();
    for reading in readings {
        strongest_by_ssid
            .entry(reading.ssid.clone())
            .and_modify(|existing| {
                if reading.strength > existing.strength {
                    *existing = reading.clone();
                }
            })
            .or_insert(reading);
    }

    let mut summaries: Vec<AccessPointSummary> = strongest_by_ssid
        .into_values()
        .map(|reading| AccessPointSummary {
            is_active: active_ssid == Some(reading.ssid.as_str()),
            saved_connection: saved.get(&reading.ssid).cloned(),
            ssid: reading.ssid,
            strength: reading.strength,
            security: reading.security,
        })
        .collect();

    summaries.sort_by(|a, b| {
        b.is_active
            .cmp(&a.is_active)
            .then_with(|| b.strength.cmp(&a.strength))
    });

    summaries
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reading(ssid: &str, strength: u8) -> RawApReading {
        RawApReading {
            ssid: ssid.to_string(),
            strength,
            security: ApSecurity::Psk,
        }
    }

    #[test]
    fn keeps_the_strongest_reading_per_ssid() {
        let readings = vec![
            reading("Home", 40),
            reading("Home", 90),
            reading("Home", 60),
        ];

        let summaries = dedupe_and_sort(readings, None, &HashMap::new());

        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].strength, 90);
    }

    #[test]
    fn sorts_active_connection_first_regardless_of_strength() {
        let readings = vec![reading("Strong", 100), reading("ActiveButWeak", 10)];

        let summaries = dedupe_and_sort(readings, Some("ActiveButWeak"), &HashMap::new());

        assert_eq!(summaries[0].ssid, "ActiveButWeak");
        assert!(summaries[0].is_active);
        assert_eq!(summaries[1].ssid, "Strong");
        assert!(!summaries[1].is_active);
    }

    #[test]
    fn sorts_remaining_networks_by_descending_strength() {
        let readings = vec![
            reading("Weak", 20),
            reading("Strongest", 90),
            reading("Mid", 50),
        ];

        let summaries = dedupe_and_sort(readings, None, &HashMap::new());

        let ssids: Vec<&str> = summaries.iter().map(|s| s.ssid.as_str()).collect();
        assert_eq!(ssids, vec!["Strongest", "Mid", "Weak"]);
    }

    #[test]
    fn marks_saved_networks() {
        let path = OwnedObjectPath::try_from("/org/freedesktop/NetworkManager/Settings/1")
            .expect("valid object path");
        let mut saved = HashMap::new();
        saved.insert("Home".to_string(), path.clone());

        let readings = vec![reading("Home", 80), reading("Guest", 60)];
        let summaries = dedupe_and_sort(readings, None, &saved);

        let home = summaries.iter().find(|s| s.ssid == "Home").unwrap();
        let guest = summaries.iter().find(|s| s.ssid == "Guest").unwrap();
        assert!(home.is_saved());
        assert_eq!(home.saved_connection, Some(path));
        assert!(!guest.is_saved());
    }
}
