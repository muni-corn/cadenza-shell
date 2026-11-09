pub mod card;
pub mod daemon;
pub mod fresh;
mod hints;
pub mod image;
mod lifecycle;
mod markup;
pub mod panel;
pub mod types;

use std::{collections::HashMap, sync::OnceLock};

use anyhow::Result;
use relm4::SharedState;
use tokio::sync::{broadcast, mpsc};
use zbus::Connection;

use crate::notifications::{
    daemon::{NotificationsDaemon, NotificationsDaemonSignals},
    lifecycle::CloseReason,
    types::Notification,
};

/// Global snapshot of all current notifications.
///
/// Consumers subscribe via `NOTIFICATIONS_STATE.subscribe(sender, mapper)` for
/// reactive updates, or read the current value with
/// `NOTIFICATIONS_STATE.read()`.
pub static NOTIFICATIONS_STATE: SharedState<NotificationsState> = SharedState::new();

/// Snapshot of the current notification state.
#[derive(Debug, Clone, Default)]
pub struct NotificationsState {
    pub notifications: HashMap<u32, Notification>,
}

/// A discrete notification event broadcast to all subscribers.
///
/// Use [`subscribe_events`] to obtain a receiver for this stream.
#[derive(Debug, Clone)]
pub enum NotificationEvent {
    Received(Notification),
    Closed { id: u32 },
}

// capacity of 64 events; lagging receivers miss old events but never block
// the producer, matching the pattern used in sleep_monitor
static EVENT_TX: OnceLock<broadcast::Sender<NotificationEvent>> = OnceLock::new();

pub(crate) fn event_tx() -> &'static broadcast::Sender<NotificationEvent> {
    EVENT_TX.get_or_init(|| broadcast::channel(64).0)
}

/// Subscribe to notification events.
///
/// Returns a receiver that yields a [`NotificationEvent`] for each change.
/// Multiple consumers can call this independently to each get their own
/// receiver.
pub fn subscribe_events() -> broadcast::Receiver<NotificationEvent> {
    event_tx().subscribe()
}

/// Commands that consumers can send to the notification service.
pub(crate) enum NotificationCommand {
    Dismiss(u32),
    ClearAll,
    InvokeAction { id: u32, action_key: String },
}

static COMMAND_TX: OnceLock<mpsc::UnboundedSender<NotificationCommand>> = OnceLock::new();

/// Dismiss a notification by ID.
///
/// Closes the notification as dismissed by the user.
/// Has no effect if the service has not been started.
pub fn dismiss(id: u32) {
    if let Some(tx) = COMMAND_TX.get() {
        let _ = tx.send(NotificationCommand::Dismiss(id));
    }
}

/// Clear all notifications.
///
/// Closes every notification as dismissed by the user.
/// Has no effect if the service has not been started.
pub fn clear_all() {
    if let Some(tx) = COMMAND_TX.get() {
        let _ = tx.send(NotificationCommand::ClearAll);
    }
}

/// Invoke a notification action, emitting the D-Bus `ActionInvoked` signal.
///
/// Has no effect if the service has not been started.
pub fn invoke_action(id: u32, action_key: String) {
    if let Some(tx) = COMMAND_TX.get() {
        let _ = tx.send(NotificationCommand::InvokeAction { id, action_key });
    }
}

/// Runs the notification service.
///
/// Registers `org.freedesktop.Notifications` on the session D-Bus, then drives
/// a command loop that handles [`dismiss`], [`clear_all`], and
/// [`invoke_action`] calls from UI components. Writes all state changes to
/// [`NOTIFICATIONS_STATE`] and broadcasts [`NotificationEvent`]s to every
/// subscriber obtained via [`subscribe_events`].
///
/// Must be started exactly once, from `app.rs`.
#[tracing::instrument(level = "debug", skip_all)]
pub async fn run_notifications_service() {
    // initialize the broadcast sender so subscribers can call
    // subscribe_events() before the first event arrives
    let _ = event_tx();

    let connection = match initialize_notifications_daemon().await {
        Ok(c) => {
            tracing::info!(
                bus_name = "org.freedesktop.Notifications",
                "notifications service registered on session bus"
            );
            c
        }
        Err(e) => {
            tracing::error!(error = %e, "failed to start notifications service");
            return;
        }
    };

    lifecycle::set_connection(connection.clone());

    // look up the interface ref so we can emit D-Bus signals for commands
    let interface_ref = match connection
        .object_server()
        .interface::<_, NotificationsDaemon>(lifecycle::OBJECT_PATH)
        .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(error = %e, "couldn't look up notifications daemon interface");
            return;
        }
    };

    // install the command sender so free functions can push commands
    let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel::<NotificationCommand>();
    if COMMAND_TX.set(cmd_tx).is_err() {
        tracing::warn!("notifications service started more than once; extra instance exiting");
        return;
    }

    tracing::debug!("entering command loop");

    // drive the command loop
    loop {
        let Some(cmd) = cmd_rx.recv().await else {
            tracing::warn!("notification command channel closed; service stopping");
            break;
        };

        match cmd {
            NotificationCommand::Dismiss(id) => {
                tracing::trace!(notification.id = id, "dismiss command received");
                lifecycle::close(id, CloseReason::Dismissed).await;
            }
            NotificationCommand::ClearAll => {
                tracing::trace!("clear_all command received");

                // close each one so clients and popups hear about it
                let ids: Vec<u32> = NOTIFICATIONS_STATE
                    .read()
                    .notifications
                    .keys()
                    .copied()
                    .collect();
                for id in ids {
                    lifecycle::close(id, CloseReason::Dismissed).await;
                }
            }
            NotificationCommand::InvokeAction { id, action_key } => {
                tracing::trace!(notification.id = id, %action_key, "invoke_action command received");

                if let Err(e) = interface_ref.action_invoked(id, action_key).await {
                    tracing::error!(notification.id = id, error = %e, "couldn't emit action_invoked signal");
                }

                // servers close notifications once an action is taken,
                // unless the client asked for them to stay
                let resident = NOTIFICATIONS_STATE
                    .read()
                    .notifications
                    .get(&id)
                    .is_some_and(|notification| notification.resident);
                if !resident {
                    lifecycle::close(id, CloseReason::Dismissed).await;
                }
            }
        }
    }
}

#[tracing::instrument(level = "debug", skip_all)]
async fn initialize_notifications_daemon() -> Result<Connection> {
    tracing::debug!("building D-Bus connection for org.freedesktop.Notifications");
    Ok(zbus::connection::Builder::session()?
        .name("org.freedesktop.Notifications")?
        .serve_at(lifecycle::OBJECT_PATH, NotificationsDaemon)?
        .build()
        .await?)
}
