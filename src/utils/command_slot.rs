use std::sync::OnceLock;

use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

/// A global channel that ui code uses to send commands to a service.
///
/// The service opens the slot once when it starts; commands sent before that
/// are dropped with a warning, since there's nobody to handle them yet.
///
/// # Example
/// ```ignore
/// static COMMANDS: CommandSlot<Command> = CommandSlot::new("network");
/// let Some(mut commands) = COMMANDS.open() else { return };
/// COMMANDS.send(Command::Scan);
/// ```
pub struct CommandSlot<T> {
    service: &'static str,
    sender: OnceLock<UnboundedSender<T>>,
}

impl<T> CommandSlot<T> {
    /// Creates an empty slot for the named service.
    pub const fn new(service: &'static str) -> Self {
        Self {
            service,
            sender: OnceLock::new(),
        }
    }

    /// Opens the slot and returns the receiving end, or `None` if it was
    /// already opened (i.e. the service was started twice).
    pub fn open(&self) -> Option<UnboundedReceiver<T>> {
        let (tx, rx) = unbounded_channel();
        self.sender.set(tx).ok()?;
        Some(rx)
    }

    /// Sends a command to the service.
    pub fn send(&self, command: T) {
        match self.sender.get() {
            Some(tx) => {
                let _ = tx.send(command);
            }
            None => tracing::warn!(
                "{} command sent before the service started; dropping it",
                self.service
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_once_and_delivers_commands() {
        let slot = CommandSlot::new("test");
        let mut rx = slot.open().expect("first open succeeds");
        assert!(slot.open().is_none());

        slot.send(7);
        assert_eq!(rx.try_recv(), Ok(7));
    }

    #[test]
    fn commands_before_opening_are_dropped() {
        let slot = CommandSlot::new("test");
        slot.send(1);
        let mut rx = slot.open().unwrap();
        assert!(rx.try_recv().is_err());
    }
}
