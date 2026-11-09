use std::{cmp::Reverse, collections::HashMap};

use tokio::task::JoinSet;
use zbus::zvariant::OwnedObjectPath;

use crate::network::{
    dbus::{
        AccessPointProxy, SettingsConnectionProxy, SettingsProxy, WirelessDeviceProxy, proxy_at,
    },
    security::ApSecurity,
    state::AccessPointSummary,
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
) -> anyhow::Result<Vec<AccessPointSummary>> {
    let wifi_proxy = proxy_at::<WirelessDeviceProxy>(conn, device_path).await?;
    let ap_paths = wifi_proxy.get_all_access_points().await?;

    // read every access point at once instead of one round trip after
    // another, which held up the service loop during scans
    let mut reads = JoinSet::new();
    for ap_path in ap_paths {
        let conn = conn.clone();
        reads.spawn(async move {
            let reading = read_access_point(&conn, &ap_path).await;
            (ap_path, reading)
        });
    }

    let mut readings = Vec::with_capacity(reads.len());
    while let Some(read) = reads.join_next().await {
        match read {
            Ok((_, Ok(Some(reading)))) => readings.push(reading),
            // empty or whitespace-only SSID (e.g. a hidden network); skip
            Ok((_, Ok(None))) => {}
            Ok((ap_path, Err(e))) => tracing::debug!("couldn't read access point {ap_path}: {e}"),
            Err(e) => tracing::debug!("access point read task failed: {e}"),
        }
    }

    let saved = fetch_saved_wifi_connections(conn)
        .await
        .unwrap_or_else(|e| {
            tracing::warn!("couldn't fetch saved wifi connections: {e}");
            HashMap::new()
        });

    Ok(dedupe_and_sort(readings, &saved))
}

/// Reads a single access point's SSID, strength, and security. Returns
/// `Ok(None)` for access points with no usable SSID (hidden networks
/// broadcast an empty or whitespace-only one).
async fn read_access_point(
    conn: &zbus::Connection,
    ap_path: &OwnedObjectPath,
) -> anyhow::Result<Option<RawApReading>> {
    let ap_proxy = proxy_at::<AccessPointProxy>(conn, ap_path).await?;

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

/// Returns saved wifi connection profiles grouped by SSID; a network can
/// have several.
async fn fetch_saved_wifi_connections(
    conn: &zbus::Connection,
) -> anyhow::Result<HashMap<String, Vec<OwnedObjectPath>>> {
    let settings_proxy = SettingsProxy::new(conn).await?;
    let connection_paths = settings_proxy.list_connections().await?;

    let mut result: HashMap<String, Vec<OwnedObjectPath>> = HashMap::new();
    for path in connection_paths {
        let Ok(conn_proxy) = proxy_at::<SettingsConnectionProxy>(conn, &path).await else {
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

        result
            .entry(String::from_utf8_lossy(&ssid).to_string())
            .or_default()
            .push(path);
    }

    Ok(result)
}

/// Groups raw per-BSSID readings by SSID (keeping only the strongest signal
/// for each), attaches saved status, and sorts by descending signal
/// strength, then by name.
fn dedupe_and_sort(
    readings: Vec<RawApReading>,
    saved: &HashMap<String, Vec<OwnedObjectPath>>,
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
            saved_connections: saved.get(&reading.ssid).cloned().unwrap_or_default(),
            ssid: reading.ssid,
            strength: reading.strength,
            security: reading.security,
        })
        .collect();

    // ties are broken by name so equal strengths don't shuffle between scans
    summaries.sort_by_key(|summary| (Reverse(summary.strength), summary.ssid.clone()));

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

        let summaries = dedupe_and_sort(readings, &HashMap::new());

        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].strength, 90);
    }

    #[test]
    fn sorts_networks_by_descending_strength() {
        let readings = vec![
            reading("Weak", 20),
            reading("Strongest", 90),
            reading("Mid", 50),
        ];

        let summaries = dedupe_and_sort(readings, &HashMap::new());

        let ssids: Vec<&str> = summaries.iter().map(|s| s.ssid.as_str()).collect();
        assert_eq!(ssids, vec!["Strongest", "Mid", "Weak"]);
    }

    #[test]
    fn breaks_strength_ties_by_name() {
        let readings = vec![reading("b", 50), reading("c", 50), reading("a", 50)];

        let summaries = dedupe_and_sort(readings, &HashMap::new());

        let ssids: Vec<&str> = summaries.iter().map(|s| s.ssid.as_str()).collect();
        assert_eq!(ssids, vec!["a", "b", "c"]);
    }

    #[test]
    fn marks_saved_networks() {
        let path = OwnedObjectPath::try_from("/org/freedesktop/NetworkManager/Settings/1")
            .expect("valid object path");
        let mut saved = HashMap::new();
        saved.insert("Home".to_string(), vec![path.clone()]);

        let readings = vec![reading("Home", 80), reading("Guest", 60)];
        let summaries = dedupe_and_sort(readings, &saved);

        let home = summaries.iter().find(|s| s.ssid == "Home").unwrap();
        let guest = summaries.iter().find(|s| s.ssid == "Guest").unwrap();
        assert!(home.is_saved());
        assert_eq!(home.saved_connections, vec![path]);
        assert!(!guest.is_saved());
    }
}
