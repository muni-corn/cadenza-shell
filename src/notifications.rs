mod card;
mod daemon;
pub mod fresh;
mod hints;
mod image;
mod lifecycle;
mod markup;
pub mod panel;
mod service;
pub mod types;

use std::collections::HashMap;

use relm4::SharedState;
pub use service::run_notifications_service;
use tokio::sync::broadcast;

use crate::{
    notifications::types::{InvokedAction, Notification},
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
enum NotificationCommand {
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
