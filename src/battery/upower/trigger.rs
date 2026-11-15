use std::{ffi::OsString, time::Duration};

use futures_lite::StreamExt;
use tokio::sync::mpsc;
use zbus::{fdo::PropertiesProxy, zvariant::OwnedObjectPath};

use super::find_battery_device;
use crate::utils::backoff::Backoff;

/// Sends on `tx` every time UPower reports a change to the battery named
/// `battery_name`, forever.
///
/// Reconnects with a backoff if UPower or the system bus goes away. Signals
/// that arrive while a refresh is already pending are dropped, so a burst of
/// changes causes a single refresh.
pub async fn run_upower_trigger(battery_name: OsString, tx: mpsc::Sender<()>) {
    let mut backoff = Backoff::new(Duration::from_secs(1), Duration::from_secs(60));
    loop {
        match watch_device(&battery_name, &tx, &mut backoff).await {
            Ok(()) => tracing::warn!("upower signal stream ended, reconnecting"),
            Err(e) => {
                tracing::warn!("couldn't watch upower for battery changes, polling meanwhile: {e}")
            }
        }
        tokio::time::sleep(backoff.next()).await;
    }
}

/// Forwards property changes until the signal stream ends.
async fn watch_device(
    battery_name: &OsString,
    tx: &mpsc::Sender<()>,
    backoff: &mut Backoff,
) -> zbus::Result<()> {
    let conn = zbus::Connection::system().await?;
    let path: OwnedObjectPath = find_battery_device(&conn, battery_name).await?;

    let properties = PropertiesProxy::builder(&conn)
        .destination("org.freedesktop.UPower")?
        .path(&path)?
        .build()
        .await?;
    let mut changes = properties.receive_properties_changed().await?;

    // subscribing worked, so the next failure starts from a short delay again
    backoff.reset();
    tracing::debug!("watching upower device {path} for battery changes");

    while changes.next().await.is_some() {
        // a full channel means a refresh is already pending
        let _ = tx.try_send(());
    }

    Ok(())
}
