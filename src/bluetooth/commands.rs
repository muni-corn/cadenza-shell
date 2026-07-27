// temporary until bluetooth_menu calls these free functions; the service
// already handles every command variant
#![allow(dead_code)]

use std::sync::OnceLock;

use bluer::Address;
use tokio::sync::mpsc::UnboundedSender;

use crate::bluetooth::agent::PairingReply;

/// Commands the UI can send to the bluetooth service.
///
/// Scoped to the operations added for scanning and pairing; connect,
/// disconnect, and power toggling already work by calling `bluer` directly
/// from the UI via `BluetoothState::device_handle` (see `bluetooth_menu.rs`)
/// and don't need a detour through the service. Discovery specifically needs
/// a persistent service-side task (the discovery stream must stay alive for
/// the whole session and auto-stop after a timeout), which a one-shot UI
/// call can't manage on its own.
#[derive(Debug)]
pub(crate) enum BluetoothCommand {
    StartDiscovery,
    StopDiscovery,
    Pair(Address),
    SetTrusted(Address, bool),
    Remove(Address),
    /// A reply to the currently pending pairing prompt, if any.
    PairingReply(PairingReply),
    /// Rejects the currently pending pairing prompt, if any.
    CancelPairing,
}

static COMMAND_TX: OnceLock<UnboundedSender<BluetoothCommand>> = OnceLock::new();

/// Installs the command sender. Called once from `run_bluetooth_service`.
///
/// Returns `Err` if called more than once (e.g. the service was somehow
/// started twice).
pub(crate) fn install_command_sender(tx: UnboundedSender<BluetoothCommand>) -> Result<(), ()> {
    COMMAND_TX.set(tx).map_err(|_| ())
}

fn send(cmd: BluetoothCommand) {
    match COMMAND_TX.get() {
        Some(tx) => {
            let _ = tx.send(cmd);
        }
        None => tracing::warn!("bluetooth command sent before the service started; dropping"),
    }
}

/// Starts a device discovery session. A no-op if one is already running.
pub fn start_discovery() {
    send(BluetoothCommand::StartDiscovery);
}

/// Stops the current device discovery session, if any.
pub fn stop_discovery() {
    send(BluetoothCommand::StopDiscovery);
}

/// Initiates pairing with a device by address.
pub fn pair(address: Address) {
    send(BluetoothCommand::Pair(address));
}

/// Sets whether a device is trusted (auto-connect/auto-authorize).
pub fn set_trusted(address: Address, trusted: bool) {
    send(BluetoothCommand::SetTrusted(address, trusted));
}

/// Removes (unpairs and forgets) a device by address.
pub fn remove(address: Address) {
    send(BluetoothCommand::Remove(address));
}

/// Responds to the current pairing prompt, if any.
pub fn pairing_reply(reply: PairingReply) {
    send(BluetoothCommand::PairingReply(reply));
}

/// Rejects the current pairing prompt, if any.
pub fn cancel_pairing() {
    send(BluetoothCommand::CancelPairing);
}
