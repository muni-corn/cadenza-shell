//! One-off operations on the adapter and its devices, requested from the
//! menu.
//!
//! Each runs as its own task: pairing in particular can take as long as the
//! user needs to answer a prompt, and awaiting it in the service loop would
//! hold up every other bluetooth event in the meantime.

use bluer::{Adapter, Address, Device};

use crate::bluetooth::state::BLUETOOTH_STATE;

/// An operation on a single device.
#[derive(Debug, Clone, Copy)]
pub enum DeviceAction {
    Connect,
    Disconnect,
    Pair,
    Remove,
}

/// Runs `action` against the device at `address` in the background.
pub fn spawn_device_action(adapter: Adapter, address: Address, action: DeviceAction) {
    relm4::spawn(async move {
        if let Err(e) = run_device_action(&adapter, address, action).await {
            tracing::warn!("couldn't {action:?} bluetooth device {address}: {e}");
        }
    });
}

async fn run_device_action(
    adapter: &Adapter,
    address: Address,
    action: DeviceAction,
) -> bluer::Result<()> {
    match action {
        DeviceAction::Connect => adapter.device(address)?.connect().await,
        DeviceAction::Disconnect => adapter.device(address)?.disconnect().await,
        DeviceAction::Pair => pair(&adapter.device(address)?).await,
        DeviceAction::Remove => adapter.remove_device(address).await,
    }
}

/// Pairs with a device and trusts it, the common convention that lets it
/// reconnect without re-authorizing.
async fn pair(device: &Device) -> bluer::Result<()> {
    device.pair().await?;
    tracing::info!("paired with {}", device.address());
    if let Err(e) = device.set_trusted(true).await {
        tracing::warn!(
            "paired with {} but couldn't trust it: {e}",
            device.address()
        );
    }
    Ok(())
}

/// Turns the adapter on or off in the background.
pub fn spawn_set_powered(adapter: Adapter, powered: bool) {
    relm4::spawn(async move {
        if let Err(e) = adapter.set_powered(powered).await {
            tracing::error!(
                "couldn't turn bluetooth {}: {e}",
                if powered { "on" } else { "off" }
            );

            // nothing changed on the bus, so republish the current state to
            // move the menu's switch back
            drop(BLUETOOTH_STATE.write());
        }
    });
}
