pub mod card;
pub mod daemon;
pub mod fresh;
mod hints;
pub mod image;
mod lifecycle;
mod markup;
pub mod panel;
pub mod types;

use std::collections::HashMap;

use anyhow::Result;
use relm4::SharedState;
use tokio::sync::broadcast;
use zbus::Connection;

use crate::{
    notifications::{
        daemon::{NotificationsDaemon, NotificationsDaemonSignals},
        lifecycle::CloseReason,
        types::{InvokedAction, Notification},
    },
    utils::{command_slot::CommandSlot, event_bus::EventBus},
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

/// Notification events for the popups and tiles.
pub(crate) static EVENTS: EventBus<NotificationEvent> = EventBus::new(64);

/// Subscribe to notification events.
///
/// Returns a receiver that yields a [`NotificationEvent`] for each change.
/// Multiple consumers can call this independently to each get their own
/// receiver.
pub fn subscribe_events() -> broadcast::Receiver<NotificationEvent> {
    EVENTS.subscribe()
}

/// Commands that consumers can send to the notification service.
pub(crate) enum NotificationCommand {
    Dismiss(u32),
    ClearAll,
    InvokeAction(InvokedAction),
}

/// Commands from the ui, received by the service loop.
static COMMANDS: CommandSlot<NotificationCommand> = CommandSlot::new("notifications");

/// Dismiss a notification by ID.
///
/// Closes the notification as dismissed by the user.
/// Has no effect if the service has not been started.
pub fn dismiss(id: u32) {
    COMMANDS.send(NotificationCommand::Dismiss(id));
}

/// Clear all notifications.
///
/// Closes every notification as dismissed by the user.
/// Has no effect if the service has not been started.
pub fn clear_all() {
    COMMANDS.send(NotificationCommand::ClearAll);
}

/// Invoke a notification action, emitting the D-Bus `ActivationToken` (if
/// there's a token) and `ActionInvoked` signals.
///
/// Has no effect if the service has not been started.
pub fn invoke_action(action: InvokedAction) {
    COMMANDS.send(NotificationCommand::InvokeAction(action));
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

    // open the command slot so free functions can push commands
    let Some(mut cmd_rx) = COMMANDS.open() else {
        tracing::warn!("notifications service started more than once; extra instance exiting");
        return;
    };

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
            NotificationCommand::InvokeAction(InvokedAction {
                id,
                key,
                activation_token,
            }) => {
                tracing::trace!(notification.id = id, action = %key, "invoke_action command received");

                // the spec sends the token first so the app has it when it
                // handles the action
                if let Some(token) = activation_token
                    && let Err(e) = interface_ref.activation_token(id, token).await
                {
                    tracing::error!(notification.id = id, error = %e, "couldn't emit activation_token signal");
                }

                if let Err(e) = interface_ref.action_invoked(id, key).await {
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
