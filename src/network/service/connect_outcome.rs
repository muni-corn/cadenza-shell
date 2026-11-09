//! Following a single connection attempt to its outcome.

use std::time::Duration;

use futures_lite::StreamExt;
use zbus::zvariant::OwnedObjectPath;

use crate::network::{
    dbus::ActiveConnectionProxy,
    events::{ConnectFailureReason, NetworkEvent},
    types::{
        NM_ACTIVE_CONNECTION_STATE_ACTIVATED, NM_ACTIVE_CONNECTION_STATE_DEACTIVATED,
        NM_ACTIVE_CONNECTION_STATE_REASON_NO_SECRETS,
    },
};

/// Watches one activation attempt until it activates or fails.
///
/// The active connection object belongs to this attempt alone, unlike the
/// device, so concurrent attempts can't be confused with each other. A
/// failure with reason `NO_SECRETS` means the password was missing, wrong,
/// or otherwise rejected; anything else is a generic failure. Returns
/// `None` if the attempt is still going when the timeout elapses (e.g. slow
/// DHCP), since that isn't a failure.
pub(super) async fn watch_connect_outcome(
    conn: &zbus::Connection,
    active_connection: &OwnedObjectPath,
    ssid: &str,
) -> anyhow::Result<Option<NetworkEvent>> {
    const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

    let succeeded = || NetworkEvent::ConnectionSucceeded {
        ssid: ssid.to_string(),
    };
    let failed = |reason| NetworkEvent::ConnectionFailed {
        ssid: ssid.to_string(),
        reason,
    };

    // read the state uncached and only after subscribing, so a transition in
    // between can't be missed
    let proxy = ActiveConnectionProxy::builder(conn)
        .path(active_connection)?
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await?;
    let mut changes = proxy.receive_active_state_changed().await?;

    match proxy.state().await {
        Ok(NM_ACTIVE_CONNECTION_STATE_ACTIVATED) => return Ok(Some(succeeded())),
        Ok(NM_ACTIVE_CONNECTION_STATE_DEACTIVATED) => {
            return Ok(Some(failed(ConnectFailureReason::Other)));
        }
        Ok(_) => {}
        // the object disappears once a failed attempt is torn down
        Err(_) => return Ok(Some(failed(ConnectFailureReason::Other))),
    }

    let outcome = tokio::time::timeout(CONNECT_TIMEOUT, async {
        while let Some(signal) = changes.next().await {
            let Ok(args) = signal.args() else {
                continue;
            };

            match args.state {
                NM_ACTIVE_CONNECTION_STATE_ACTIVATED => return Some(succeeded()),
                NM_ACTIVE_CONNECTION_STATE_DEACTIVATED => {
                    let reason = if args.reason == NM_ACTIVE_CONNECTION_STATE_REASON_NO_SECRETS {
                        ConnectFailureReason::WrongPassword
                    } else {
                        ConnectFailureReason::Other
                    };
                    return Some(failed(reason));
                }
                // still activating or deactivating
                _ => continue,
            }
        }
        // the object went away without a final state change
        Some(failed(ConnectFailureReason::Other))
    })
    .await;

    match outcome {
        Ok(event) => Ok(event),
        Err(_) => {
            tracing::debug!("{ssid} is still connecting after {CONNECT_TIMEOUT:?}");
            Ok(None)
        }
    }
}
