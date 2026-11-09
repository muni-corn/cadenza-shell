use std::time::Duration;

use futures_lite::StreamExt;
use tokio::sync::broadcast;
use zbus::proxy;

use crate::utils::{backoff::Backoff, event_bus::EventBus};

/// Fires each time the system wakes from sleep.
static WAKE: EventBus<()> = EventBus::new(8);

/// Subscribe to system wake events.
///
/// Returns a receiver that yields `()` each time the system wakes from sleep.
/// Multiple services can call this independently to each get their own
/// receiver.
pub fn subscribe_wake() -> broadcast::Receiver<()> {
    WAKE.subscribe()
}

#[proxy(
    interface = "org.freedesktop.login1.Manager",
    default_service = "org.freedesktop.login1",
    default_path = "/org/freedesktop/login1"
)]
trait Login1Manager {
    /// Signal emitted before sleep (`start = true`) and after wake (`start =
    /// false`).
    #[zbus(signal)]
    fn prepare_for_sleep(&self, start: bool) -> zbus::Result<()>;
}

/// Subscribes to the logind `PrepareForSleep` D-Bus signal and broadcasts wake
/// events to all subscribers obtained via [`subscribe_wake`].
///
/// Reconnects with a backoff if the system bus connection or the signal
/// stream goes away.
pub async fn run_sleep_monitor() {
    let mut backoff = Backoff::new(Duration::from_secs(1), Duration::from_secs(60));
    loop {
        match watch_sleep().await {
            Ok(()) => tracing::warn!("PrepareForSleep signal stream ended, reconnecting"),
            Err(e) => tracing::warn!("sleep monitor failed, reconnecting: {e}"),
        }
        tokio::time::sleep(backoff.next()).await;
    }
}

/// Forwards wake events until the signal stream ends.
async fn watch_sleep() -> zbus::Result<()> {
    let conn = zbus::Connection::system().await?;
    let proxy = Login1ManagerProxy::new(&conn).await?;
    let mut stream = proxy.receive_prepare_for_sleep().await?;

    while let Some(signal) = stream.next().await {
        let args = match signal.args() {
            Ok(args) => args,
            Err(e) => {
                tracing::warn!("couldn't read PrepareForSleep arguments: {e}");
                continue;
            }
        };

        // start=false means the system has just woken up
        if !args.start {
            tracing::debug!("system wake detected, notifying subscribers");
            WAKE.send(());
        }
    }

    Ok(())
}
