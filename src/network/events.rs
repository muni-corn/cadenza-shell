use tokio::sync::broadcast;

use crate::utils::event_bus::EventBus;

/// The outcome of a connection attempt, broadcast so the menu can show
/// specific feedback (e.g. distinguishing a wrong password from a generic
/// failure) instead of a connect button that silently does nothing.
#[derive(Debug, Clone)]
pub enum NetworkEvent {
    ConnectionSucceeded {
        ssid: String,
    },
    ConnectionFailed {
        ssid: String,
        reason: ConnectFailureReason,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectFailureReason {
    /// NetworkManager reported `NO_SECRETS`: the password was missing,
    /// wrong, or otherwise rejected.
    WrongPassword,
    /// Any other failure (out of range, DHCP failure, association timeout,
    /// etc), or we simply timed out waiting to find out.
    Other,
}

/// Connection attempt outcomes for the menu.
pub(crate) static EVENTS: EventBus<NetworkEvent> = EventBus::new(16);

/// Subscribes to network connection-attempt outcomes.
///
/// Returns a receiver that yields a [`NetworkEvent`] for each connect
/// attempt's result. Multiple consumers can call this independently to each
/// get their own receiver.
pub fn subscribe_events() -> broadcast::Receiver<NetworkEvent> {
    EVENTS.subscribe()
}
