// temporary until network_menu subscribes to these events
#![allow(dead_code)]

use std::sync::OnceLock;

use tokio::sync::broadcast;

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

// capacity of 16 events; lagging receivers miss old events but never block
// the producer, matching the pattern used in sleep_monitor/notifications
static EVENT_TX: OnceLock<broadcast::Sender<NetworkEvent>> = OnceLock::new();

pub(crate) fn event_tx() -> &'static broadcast::Sender<NetworkEvent> {
    EVENT_TX.get_or_init(|| broadcast::channel(16).0)
}

/// Subscribes to network connection-attempt outcomes.
///
/// Returns a receiver that yields a [`NetworkEvent`] for each connect
/// attempt's result. Multiple consumers can call this independently to each
/// get their own receiver.
pub fn subscribe_events() -> broadcast::Receiver<NetworkEvent> {
    event_tx().subscribe()
}
