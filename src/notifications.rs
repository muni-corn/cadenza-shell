pub mod card;
pub mod daemon;
pub mod fresh;
pub mod panel;
pub mod types;

use std::{collections::HashMap, sync::OnceLock};

use anyhow::Result;
use relm4::SharedState;
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, mpsc};
use zbus::{
    Connection,
    zvariant::{
        OwnedValue, Type,
        as_value::{self, optional},
    },
};

use crate::notifications::{
    daemon::{NotificationsDaemon, NotificationsDaemonSignals},
    types::{Notification, NotificationUrgency},
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
    Closed {
        id: u32,
        // reason codes defined by the freedesktop spec; retained even if
        // consumers currently pattern-match with `..`
        #[allow(dead_code)]
        reason: u32,
    },
    ActionInvoked {
        // retained for future consumers; currently matched with `..`
        #[allow(dead_code)]
        id: u32,
        #[allow(dead_code)]
        action_key: String,
    },
    AllCleared,
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
/// Removes the notification from state and emits a `NotificationClosed` event.
/// Has no effect if the service has not been started.
pub fn dismiss(id: u32) {
    if let Some(tx) = COMMAND_TX.get() {
        let _ = tx.send(NotificationCommand::Dismiss(id));
    }
}

/// Clear all notifications.
///
/// Removes all notifications from state and emits an `AllCleared` event.
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

/// D-Bus hints passed with each `Notify` call.
#[derive(Deserialize, Serialize, Type, Default)]
#[zvariant(signature = "dict")]
#[serde(default, rename_all = "kebab-case")]
pub struct NotificationHints {
    #[serde(with = "as_value")]
    action_icons: bool,

    #[serde(with = "optional", skip_serializing_if = "Option::is_none")]
    category: Option<String>,

    #[serde(with = "optional", skip_serializing_if = "Option::is_none")]
    pub desktop_entry: Option<String>,

    #[serde(with = "as_value")]
    resident: bool,

    #[serde(with = "optional", skip_serializing_if = "Option::is_none")]
    sound_file: Option<String>,

    #[serde(with = "optional", skip_serializing_if = "Option::is_none")]
    sound_name: Option<String>,

    #[serde(with = "as_value")]
    suppress_sound: bool,

    #[serde(with = "as_value")]
    transient: bool,

    #[serde(with = "optional", skip_serializing_if = "Option::is_none")]
    pub urgency: Option<NotificationUrgency>,

    #[serde(flatten)]
    others: HashMap<String, OwnedValue>,
}

/// Runs the notification service.
///
/// Registers `org.freedesktop.Notifications` on the session D-Bus, then drives
/// a command loop that handles [`dismiss`], [`clear_all`], and
/// [`invoke_action`] calls from UI components. Writes all state changes to
/// [`NOTIFICATIONS_STATE`] and broadcasts [`NotificationEvent`]s to every
/// subscriber obtained via [`subscribe_events`].
///
/// Must be started exactly once, from `app.rs`, before any UI component
/// subscribes to the state or issues commands.
#[tracing::instrument(level = "debug", skip_all)]
pub async fn run_notifications_service() {
    // initialize the broadcast sender so subscribers can call subscribe_events()
    // before the first event arrives
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

    // look up the interface ref so we can emit D-Bus signals for commands
    let interface_ref = match connection
        .object_server()
        .interface::<_, NotificationsDaemon>("/org/freedesktop/Notifications")
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

                NOTIFICATIONS_STATE.write().notifications.remove(&id);
                let remaining = NOTIFICATIONS_STATE.read().notifications.len();
                tracing::debug!(notification.id = id, remaining, "notification dismissed");

                let _ = event_tx().send(NotificationEvent::Closed { id, reason: 2 });

                // also emit the D-Bus signal so external clients are notified
                if let Err(e) = interface_ref.notification_closed(id, 2).await {
                    tracing::error!(notification.id = id, error = %e, "couldn't emit notification_closed signal");
                }
            }
            NotificationCommand::ClearAll => {
                tracing::trace!("clear_all command received");

                NOTIFICATIONS_STATE.write().notifications.clear();
                tracing::debug!("all notifications cleared from state");

                let _ = event_tx().send(NotificationEvent::AllCleared);
            }
            NotificationCommand::InvokeAction { id, action_key } => {
                tracing::trace!(notification.id = id, %action_key, "invoke_action command received");

                let _ = event_tx().send(NotificationEvent::ActionInvoked {
                    id,
                    action_key: action_key.clone(),
                });

                if let Err(e) = interface_ref.action_invoked(id, action_key).await {
                    tracing::error!(notification.id = id, error = %e, "couldn't emit action_invoked signal");
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
        .serve_at(
            "/org/freedesktop/Notifications",
            NotificationsDaemon::new(event_tx().clone()),
        )?
        .build()
        .await?)
}
