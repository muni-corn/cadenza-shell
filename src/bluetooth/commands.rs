use std::sync::OnceLock;

use bluer::Address;
use tokio::sync::mpsc::UnboundedSender;

use crate::bluetooth::actions::DeviceAction;

/// Commands the UI can send to the bluetooth service.
///
/// Every adapter operation goes through the service, which owns the adapter,
/// so the UI never does D-Bus I/O itself.
#[derive(Debug)]
pub(crate) enum BluetoothCommand {
    StartDiscovery,
    StopDiscovery,
    SetPowered(bool),
    Device(Address, DeviceAction),
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

/// Turns the adapter on or off.
pub fn set_powered(powered: bool) {
    send(BluetoothCommand::SetPowered(powered));
}

/// Connects to a paired device by address.
pub fn connect(address: Address) {
    send(BluetoothCommand::Device(address, DeviceAction::Connect));
}

/// Disconnects a device by address.
pub fn disconnect(address: Address) {
    send(BluetoothCommand::Device(address, DeviceAction::Disconnect));
}

/// Initiates pairing with a device by address.
pub fn pair(address: Address) {
    send(BluetoothCommand::Device(address, DeviceAction::Pair));
}

/// Removes (unpairs and forgets) a device by address.
pub fn remove(address: Address) {
    send(BluetoothCommand::Device(address, DeviceAction::Remove));
}
