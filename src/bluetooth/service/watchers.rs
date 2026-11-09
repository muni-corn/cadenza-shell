//! The agent registration and the watchers that live as long as the service.

use bluer::{Session, agent::AgentHandle};
use futures_lite::StreamExt;
use tokio::{sync::mpsc::UnboundedSender, task::AbortHandle};

use super::BluetoothEvent;
use crate::{bluetooth::agent, settings};

/// Registers the pairing agent unless it's already registered.
///
/// If becoming the default agent fails (e.g. another agent holds that role),
/// registration is retried as a non-default agent, which still handles the
/// pairings this shell starts itself.
pub(super) async fn ensure_agent(session: &Session, handle: &mut Option<AgentHandle>) {
    if handle.is_some() {
        return;
    }

    let default_agent = settings::get_config().bluetooth.default_agent;
    let registered = match agent::register_agent(session, default_agent).await {
        Err(e) if default_agent => {
            tracing::warn!("couldn't become the default pairing agent, registering normally: {e}");
            agent::register_agent(session, false).await
        }
        result => result,
    };

    match registered {
        Ok(agent_handle) => {
            tracing::info!("bluetooth pairing agent registered");
            *handle = Some(agent_handle);
        }
        Err(e) => tracing::warn!("couldn't register bluetooth pairing agent: {e}"),
    }
}

/// Sends [`BluetoothEvent::BluezRestarted`] whenever `org.bluez` gets a new
/// owner.
pub(super) fn spawn_bluez_owner_watcher(event_tx: UnboundedSender<BluetoothEvent>) -> AbortHandle {
    relm4::spawn(async move {
        let result: anyhow::Result<()> = async {
            let conn = zbus::Connection::system().await?;
            let dbus = zbus::fdo::DBusProxy::new(&conn).await?;
            let mut changes = dbus.receive_name_owner_changed().await?;
            while let Some(change) = changes.next().await {
                let args = change.args()?;
                if args.name.as_str() == "org.bluez" && args.new_owner.is_some() {
                    event_tx.send(BluetoothEvent::BluezRestarted)?;
                }
            }
            Ok(())
        }
        .await;

        if let Err(e) = result {
            tracing::warn!("stopped watching for bluetoothd restarts: {e}");
        }
    })
    .abort_handle()
}

/// Watches for adapters appearing or disappearing and forwards them as
/// [`BluetoothEvent::Session`] events.
pub(super) fn spawn_session_watcher(
    session: Session,
    event_tx: UnboundedSender<BluetoothEvent>,
) -> AbortHandle {
    relm4::spawn(async move {
        let stream = match session.events().await {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("couldn't subscribe to bluetooth session events: {e}");
                return;
            }
        };
        // Session::events()'s stream isn't Unpin, unlike Adapter/Device's;
        // pin it to a stack slot so StreamExt::next() can be called on it
        let mut stream = std::pin::pin!(stream);

        while let Some(event) = stream.next().await {
            tracing::debug!("bluetooth session event: {event:?}");
            if event_tx.send(BluetoothEvent::Session(event)).is_err() {
                break;
            }
        }
        tracing::warn!("stream for bluetooth session events has closed");
    })
    .abort_handle()
}
