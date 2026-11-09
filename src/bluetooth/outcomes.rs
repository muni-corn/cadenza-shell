//! How the operations requested from the menu turned out.

use bluer::{Address, ErrorKind};

use crate::{bluetooth::actions::DeviceAction, utils::event_bus::EventBus};

/// Where an operation on a device is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionProgress {
    Started,
    Succeeded,
    /// The operation failed, with a reason that can be shown to the user.
    Failed(String),
}

/// How an operation requested from the menu turned out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BluetoothOutcome {
    Device {
        address: Address,
        action: DeviceAction,
        progress: ActionProgress,
    },
    /// Turning the adapter on or off failed, with a reason for the user.
    PowerFailed { powered: bool, reason: String },
}

/// Outcomes of menu-requested operations, for the menus to show.
pub(crate) static OUTCOMES: EventBus<BluetoothOutcome> = EventBus::new(16);

/// Subscribes to the outcomes of operations requested from the menu.
pub fn subscribe_outcomes() -> tokio::sync::broadcast::Receiver<BluetoothOutcome> {
    OUTCOMES.subscribe()
}

/// Reports how an operation on the device at `address` is going.
pub(super) fn report(address: Address, action: DeviceAction, progress: ActionProgress) {
    OUTCOMES.send(BluetoothOutcome::Device {
        address,
        action,
        progress,
    });
}

/// Explains a bluez error in terms the user can act on.
pub(super) fn friendly_reason(error: &bluer::Error) -> String {
    // bluez reports most connection failures as a generic `Failed`, with
    // the actual cause only in the message
    let message = error.message.to_lowercase();
    if message.contains("page-timeout") || message.contains("host is down") {
        return "the device is out of range or turned off".into();
    }
    if message.contains("profile-unavailable") {
        return "the device has nothing this computer can connect to".into();
    }

    match &error.kind {
        ErrorKind::AuthenticationCanceled => "pairing was cancelled".into(),
        ErrorKind::AuthenticationFailed => "the code didn't match".into(),
        ErrorKind::AuthenticationRejected => "the device rejected pairing".into(),
        ErrorKind::AuthenticationTimeout => "the device didn't respond in time".into(),
        ErrorKind::ConnectionAttemptFailed => "the device didn't respond".into(),
        ErrorKind::AlreadyConnected | ErrorKind::AlreadyExists => "it's already done".into(),
        ErrorKind::InProgress => "it's already in progress".into(),
        ErrorKind::DoesNotExist | ErrorKind::NotFound => "the device is gone".into(),
        ErrorKind::NotReady => "the adapter isn't ready; is bluetooth on?".into(),
        ErrorKind::NotAuthorized | ErrorKind::NotPermitted => "it isn't allowed".into(),
        ErrorKind::NotSupported => "the device doesn't support it".into(),
        _ if error.message.is_empty() => format!("{error}"),
        _ => error.message.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn error(kind: ErrorKind, message: &str) -> bluer::Error {
        bluer::Error {
            kind,
            message: message.into(),
        }
    }

    #[test]
    fn generic_failures_are_explained_by_their_message() {
        let reason = friendly_reason(&error(ErrorKind::Failed, "br-connection-page-timeout"));
        assert_eq!(reason, "the device is out of range or turned off");
    }

    #[test]
    fn known_kinds_get_friendly_reasons() {
        let reason = friendly_reason(&error(ErrorKind::AuthenticationRejected, ""));
        assert_eq!(reason, "the device rejected pairing");
    }

    #[test]
    fn unknown_failures_keep_bluez_message() {
        let reason = friendly_reason(&error(ErrorKind::Failed, "Input/output error"));
        assert_eq!(reason, "Input/output error");
    }
}
