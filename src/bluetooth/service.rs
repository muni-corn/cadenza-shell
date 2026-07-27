use std::collections::HashMap;

use bluer::{
    Adapter, AdapterEvent, AdapterProperty, Address, Device, DeviceEvent, DeviceProperty, Session,
};
use futures_lite::StreamExt;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use crate::{
    bluetooth::state::{BLUETOOTH_STATE, BluetoothState, DeviceInfo},
    sleep_monitor,
};

#[derive(Debug)]
pub enum BluetoothEvent {
    Adapter(AdapterEvent),
    Device(Address, DeviceEvent),
    /// The system has just woken from sleep; triggers a full state refresh.
    Wake,
}

pub async fn run_bluetooth_service() {
    let Ok(session) = Session::new()
        .await
        .inspect_err(|e| tracing::error!("couldn't initialize bluetooth session: {e}"))
    else {
        return;
    };

    let Ok(adapter) = session
        .default_adapter()
        .await
        .inspect_err(|e| tracing::error!("couldn't get default bluetooth adapter: {e}"))
    else {
        return;
    };

    let mut devices = HashMap::new();
    if let Ok(addresses) = adapter.device_addresses().await {
        for address in addresses {
            let Ok(device) = adapter.device(address) else {
                continue;
            };
            devices.insert(address, build_device_info(&device).await);
        }
    };

    let state = BluetoothState {
        _session: session,
        powered: adapter.is_powered().await.unwrap_or(false),
        devices,
        discovering: adapter.is_discovering().await.unwrap_or(false),
        adapter: adapter.clone(),
    };

    *BLUETOOTH_STATE.write() = Some(state);

    // set up bluetooth monitoring
    let Ok((event_tx, mut event_rx, mut device_event_tasks)) = start_event_listening(adapter)
        .await
        .inspect_err(|e| tracing::error!("failed to setup bluetooth monitoring: {e}"))
    else {
        return;
    };

    // subscribe to system wake events and forward them into the event channel
    let mut wake_rx = sleep_monitor::subscribe_wake();
    let event_tx_wake = event_tx.clone();
    relm4::spawn(async move {
        loop {
            match wake_rx.recv().await {
                Ok(()) => {
                    event_tx_wake
                        .send(BluetoothEvent::Wake)
                        .unwrap_or_else(|e| tracing::error!("couldn't send wake event: {e}"));
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!("bluetooth wake receiver lagged, missed {n} wake event(s)");
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    while let Some(event) = event_rx.recv().await {
        update(event, &event_tx, &mut device_event_tasks).await;
    }
    tracing::warn!("bluetooth service has stopped receiving events");
}

async fn start_event_listening(
    adapter: Adapter,
) -> anyhow::Result<(
    UnboundedSender<BluetoothEvent>,
    UnboundedReceiver<BluetoothEvent>,
    HashMap<Address, tokio::task::AbortHandle>,
)> {
    let (event_tx, event_rx) = unbounded_channel();

    // monitor adapter events
    let mut adapter_events = adapter.events().await?;
    let event_tx_clone = event_tx.clone();
    relm4::spawn(async move {
        while let Some(event) = adapter_events.next().await {
            event_tx_clone
                .send(BluetoothEvent::Adapter(event))
                .unwrap_or_else(|e| tracing::error!("couldn't send adapter bluetooth event: {e}"));
        }
        tracing::error!("bluetooth service has stopped receiving adapter events");
    });

    // monitor existing devices for connection status changes
    let mut device_event_tasks = HashMap::new();
    let devices = adapter.device_addresses().await.unwrap_or_default();
    for addr in devices {
        if let Ok(device) = adapter.device(addr)
            && let Some(handle) = subscribe_device_events(addr, device, &event_tx).await
        {
            device_event_tasks.insert(addr, handle);
        }
    }

    Ok((event_tx, event_rx, device_event_tasks))
}

async fn update(
    input: BluetoothEvent,
    event_tx: &UnboundedSender<BluetoothEvent>,
    device_event_tasks: &mut HashMap<Address, tokio::task::AbortHandle>,
) {
    match input {
        BluetoothEvent::Wake => {
            refresh_state_after_wake().await;
        }
        // a newly added device needs its full property snapshot fetched
        // before it can be inserted, which requires an async round trip
        // that update_from_event's synchronous write-lock section can't do
        BluetoothEvent::Adapter(AdapterEvent::DeviceAdded(address)) => {
            handle_device_added(address, event_tx, device_event_tasks).await;
        }
        // abort the now-orphaned event subscription before applying the
        // sync state removal; otherwise the task leaks and keeps forwarding
        // events for a device that's no longer tracked
        BluetoothEvent::Adapter(AdapterEvent::DeviceRemoved(address)) => {
            if let Some(handle) = device_event_tasks.remove(&address) {
                handle.abort();
            }
            update_from_event(BluetoothEvent::Adapter(AdapterEvent::DeviceRemoved(
                address,
            )));
        }
        other => {
            // sync, so the write lock is always released before any
            // subsequent async subscription work
            update_from_event(other);
        }
    }
}

/// Fetches a full property snapshot for a newly discovered device, inserts
/// it into [`BLUETOOTH_STATE`], and subscribes to its future property
/// changes.
async fn handle_device_added(
    address: Address,
    event_tx: &UnboundedSender<BluetoothEvent>,
    device_event_tasks: &mut HashMap<Address, tokio::task::AbortHandle>,
) {
    let adapter = {
        let state = BLUETOOTH_STATE.read();
        let Some(ref state) = *state else {
            return;
        };
        state.adapter.clone()
    };

    let Ok(device) = adapter.device(address) else {
        return;
    };
    let info = build_device_info(&device).await;

    {
        let mut guard = BLUETOOTH_STATE.write();
        if let Some(ref mut state) = *guard {
            state.devices.insert(address, info);
        }
    }

    // replace any stale subscription for this address (e.g. it was
    // previously removed and re-added) rather than leaking it
    if let Some(handle) = subscribe_device_events(address, device, event_tx).await
        && let Some(old_handle) = device_event_tasks.insert(address, handle)
    {
        old_handle.abort();
    }
}

/// Fetches a full property snapshot for a device.
async fn build_device_info(device: &Device) -> DeviceInfo {
    DeviceInfo {
        address: device.address(),
        alias: device
            .alias()
            .await
            .unwrap_or_else(|_| device.address().to_string()),
        icon: device.icon().await.unwrap_or(None),
        paired: device.is_paired().await.unwrap_or(false),
        trusted: device.is_trusted().await.unwrap_or(false),
        connected: device.is_connected().await.unwrap_or(false),
        rssi: device.rssi().await.unwrap_or(None),
        battery_percentage: device.battery_percentage().await.unwrap_or(None),
    }
}

/// Re-polls adapter powered/discovering state and refreshes every device's
/// property snapshot after a system wake, since D-Bus events may have been
/// missed during sleep.
async fn refresh_state_after_wake() {
    tracing::debug!("system wake: refreshing bluetooth state");

    // clone the adapter and known addresses without holding the write lock
    // across the async re-fetch below
    let (adapter, addresses) = {
        let state = BLUETOOTH_STATE.read();
        let Some(ref state) = *state else {
            return;
        };
        (
            state.adapter.clone(),
            state.devices.keys().copied().collect::<Vec<_>>(),
        )
    };

    let powered = adapter.is_powered().await.unwrap_or(false);
    let discovering = adapter.is_discovering().await.unwrap_or(false);

    let mut devices = HashMap::with_capacity(addresses.len());
    for address in addresses {
        let Ok(device) = adapter.device(address) else {
            continue;
        };
        devices.insert(address, build_device_info(&device).await);
    }

    if let Some(ref mut state) = *BLUETOOTH_STATE.write() {
        state.powered = powered;
        state.discovering = discovering;
        state.devices = devices;
    }
}

/// Applies a bluetooth event to [`BLUETOOTH_STATE`] synchronously.
///
/// Handles everything except newly-added devices, which need an async
/// property fetch before they can be snapshotted (see
/// [`handle_device_added`]).
fn update_from_event(input: BluetoothEvent) {
    let mut guard = BLUETOOTH_STATE.write();
    let Some(state) = (*guard).as_mut() else {
        return;
    };

    tracing::debug!("updating bluetooth state with event: {:?}", input);

    match input {
        BluetoothEvent::Adapter(adapter_event) => match adapter_event {
            AdapterEvent::DeviceAdded(_) => {
                unreachable!("DeviceAdded is handled by handle_device_added")
            }
            AdapterEvent::DeviceRemoved(address) => {
                state.devices.remove(&address);
            }
            AdapterEvent::PropertyChanged(adapter_property) => match adapter_property {
                AdapterProperty::Powered(p) => state.powered = p,
                AdapterProperty::Discovering(d) => state.discovering = d,
                p => tracing::warn!("unhandled AdapterProperty event: {p:?}"),
            },
        },
        // wake is handled before this function is called
        BluetoothEvent::Wake => {}
        BluetoothEvent::Device(address, device_event) => {
            let Some(info) = state.devices.get_mut(&address) else {
                tracing::debug!("property change for untracked device {address}, ignoring");
                return;
            };

            match device_event {
                DeviceEvent::PropertyChanged(DeviceProperty::Connected(connected)) => {
                    info.connected = connected;
                    tracing::debug!("device {address} connected={connected}");
                }
                DeviceEvent::PropertyChanged(DeviceProperty::Alias(alias)) => info.alias = alias,
                DeviceEvent::PropertyChanged(DeviceProperty::Icon(icon)) => info.icon = Some(icon),
                DeviceEvent::PropertyChanged(DeviceProperty::Paired(paired)) => {
                    info.paired = paired
                }
                DeviceEvent::PropertyChanged(DeviceProperty::Trusted(trusted)) => {
                    info.trusted = trusted
                }
                DeviceEvent::PropertyChanged(DeviceProperty::Rssi(rssi)) => info.rssi = Some(rssi),
                DeviceEvent::PropertyChanged(DeviceProperty::BatteryPercentage(pct)) => {
                    info.battery_percentage = Some(pct)
                }
                DeviceEvent::PropertyChanged(device_property) => {
                    tracing::debug!("device {address} property changed: {device_property:?}");
                }
            }
        }
    }
}

/// Subscribes to BlueZ property change events for a device and forwards them
/// into the shared event channel.
///
/// Returns the spawned task's abort handle so the caller can cancel it once
/// the device is removed, avoiding a task leak that would otherwise keep a
/// dead subscription running (and, since bluer's BlueZ object paths can be
/// reused, potentially forwarding events for a stale device instance).
async fn subscribe_device_events(
    address: Address,
    device: Device,
    event_tx: &UnboundedSender<BluetoothEvent>,
) -> Option<tokio::task::AbortHandle> {
    match device.events().await {
        Ok(mut device_events) => {
            let tx = event_tx.clone();
            let handle = relm4::spawn(async move {
                while let Some(event) = device_events.next().await {
                    tx.send(BluetoothEvent::Device(address, event))
                        .unwrap_or_else(|e| {
                            tracing::error!(
                                "couldn't send device bluetooth event for {address}: {e}"
                            )
                        });
                }
                tracing::warn!("bluetooth event stream ended for device {address}");
            });
            Some(handle.abort_handle())
        }
        Err(e) => {
            tracing::warn!("couldn't subscribe to events for device {address}: {e}");
            None
        }
    }
}
