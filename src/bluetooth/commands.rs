use bluer::Address;

use crate::{bluetooth::actions::DeviceAction, utils::command_slot::CommandSlot};

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

/// Commands from the ui, received by the service loop.
pub(crate) static COMMANDS: CommandSlot<BluetoothCommand> = CommandSlot::new("bluetooth");

fn send(cmd: BluetoothCommand) {
    COMMANDS.send(cmd);
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
