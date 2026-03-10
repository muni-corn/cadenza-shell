use std::sync::OnceLock;

use futures_lite::StreamExt;
use tokio::sync::broadcast;
use zbus::proxy;

static WAKE_TX: OnceLock<broadcast::Sender<()>> = OnceLock::new();

fn wake_tx() -> &'static broadcast::Sender<()> {
    WAKE_TX.get_or_init(|| broadcast::channel(8).0)
}

/// Subscribe to system wake events.
///
/// Returns a receiver that yields `()` each time the system wakes from sleep.
/// Multiple services can call this independently to each get their own
/// receiver.
pub fn subscribe_wake() -> broadcast::Receiver<()> {
    wake_tx().subscribe()
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
pub async fn run_sleep_monitor() {
    let conn = match zbus::Connection::system().await {
        Ok(c) => c,
        Err(e) => {
            log::error!("couldn't connect to system D-Bus for sleep monitor: {e}");
            return;
        }
    };

    let proxy = match Login1ManagerProxy::new(&conn).await {
        Ok(p) => p,
        Err(e) => {
            log::error!("couldn't create login1 manager proxy: {e}");
            return;
        }
    };

    let mut stream = match proxy.receive_prepare_for_sleep().await {
        Ok(s) => s,
        Err(e) => {
            log::error!("couldn't subscribe to PrepareForSleep signal: {e}");
            return;
        }
    };

    while let Some(signal) = stream.next().await {
        if let Ok(args) = signal.args() {
            // start=false means the system has just woken up
            if !args.start {
                log::debug!("system wake detected, notifying subscribers");
                // ignore send errors; no subscribers is fine
                let _ = wake_tx().send(());
            }
        }
    }

    log::warn!("sleep monitor has stopped receiving PrepareForSleep signals");
}
