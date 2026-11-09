use std::sync::OnceLock;

use tokio::sync::broadcast;

/// A global broadcast channel for events from a service.
///
/// The channel is created on first use, so subscribers and the service can
/// start in any order. Subscribers that fall behind miss old events rather
/// than blocking the sender.
///
/// # Example
/// ```ignore
/// static WAKE: EventBus<()> = EventBus::new(8);
/// let mut rx = WAKE.subscribe();
/// WAKE.send(());
/// ```
pub struct EventBus<T> {
    capacity: usize,
    sender: OnceLock<broadcast::Sender<T>>,
}

impl<T: Clone> EventBus<T> {
    /// Creates a bus that keeps up to `capacity` events for slow
    /// subscribers.
    pub const fn new(capacity: usize) -> Self {
        Self {
            capacity,
            sender: OnceLock::new(),
        }
    }

    /// Sends an event to every current subscriber. Having none is fine.
    pub fn send(&self, event: T) {
        let _ = self.sender().send(event);
    }

    /// Returns a receiver for events sent from now on.
    pub fn subscribe(&self) -> broadcast::Receiver<T> {
        self.sender().subscribe()
    }

    fn sender(&self) -> &broadcast::Sender<T> {
        self.sender
            .get_or_init(|| broadcast::channel(self.capacity).0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delivers_to_subscribers() {
        let bus = EventBus::new(4);
        let mut rx = bus.subscribe();
        bus.send(3);
        assert_eq!(rx.try_recv(), Ok(3));
    }

    #[test]
    fn sending_without_subscribers_is_fine() {
        let bus = EventBus::new(4);
        bus.send(1);
        let mut rx = bus.subscribe();
        assert!(rx.try_recv().is_err());
    }
}
