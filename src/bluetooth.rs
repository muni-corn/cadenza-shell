use std::collections::{HashMap, hash_map};

use bluer::{
    Adapter, AdapterEvent, AdapterProperty, Address, Device, DeviceEvent, DeviceProperty, Session,
};
use futures_lite::StreamExt;
use relm4::SharedState;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};

use crate::sleep_monitor;

pub static BLUETOOTH_STATE: SharedState<Option<BluetoothState>> = SharedState::new();

#[derive(Debug)]
pub enum BluetoothEvent {
    Adapter(AdapterEvent),
    Device(Address, DeviceEvent),
    /// The system has just woken from sleep; triggers a full state refresh.
    Wake,
}

#[derive(Clone, Debug)]
pub struct BluetoothState {
    _session: Session,
    pub adapter: Adapter,
    devices: HashMap<Address, Device>,
    pub discovering: bool,

    pub powered: bool,
    pub connected_device_count: u8,
}

impl BluetoothState {
    pub fn devices(&self) -> hash_map::Values<'_, Address, Device> {
        self.devices.values()
    }

    pub fn get_device(&self, address: &Address) -> Option<&Device> {
        self.devices.get(address)
    }
}

pub async fn run_bluetooth_service() {
    let Ok(session) = Session::new()
        .await
        .inspect_err(|e| log::error!("couldn't initialize bluetooth session: {e}"))
    else {
        return;
    };

    let Ok(adapter) = session
        .default_adapter()
        .await
        .inspect_err(|e| log::error!("couldn't get default bluetooth adapter: {e}"))
    else {
        return;
    };

    let mut devices = HashMap::new();
    if let Ok(addresses) = adapter.device_addresses().await {
        for address in addresses {
            let Ok(device) = adapter.device(address) else {
                continue;
            };
            devices.insert(address, device);
        }
    };

    // poll initial connected count once; subsequent changes are tracked
    // incrementally via DeviceProperty::Connected events
    let mut connected_device_count: u8 = 0;
    for device in devices.values() {
        if device.is_connected().await.unwrap_or(false) {
            connected_device_count = connected_device_count.saturating_add(1);
        }
    }

    let state = BluetoothState {
        _session: session,
        powered: adapter.is_powered().await.unwrap_or(false),
        connected_device_count,
        devices,
        discovering: adapter.is_discovering().await.unwrap_or(false),
        adapter: adapter.clone(),
    };

    *BLUETOOTH_STATE.write() = Some(state);

    // set up bluetooth monitoring
    let Ok((event_tx, mut event_rx)) = start_event_listening(adapter)
        .await
        .inspect_err(|e| log::error!("failed to setup bluetooth monitoring: {e}"))
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
                        .unwrap_or_else(|e| log::error!("couldn't send wake event: {e}"));
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    log::warn!("bluetooth wake receiver lagged, missed {n} wake event(s)");
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    while let Some(event) = event_rx.recv().await {
        update(event, &event_tx).await;
    }
    log::warn!("bluetooth service has stopped receiving events");
}

async fn start_event_listening(
    adapter: Adapter,
) -> anyhow::Result<(
    UnboundedSender<BluetoothEvent>,
    UnboundedReceiver<BluetoothEvent>,
)> {
    let (event_tx, event_rx) = unbounded_channel();

    // monitor adapter events
    let mut adapter_events = adapter.events().await?;
    let event_tx_clone = event_tx.clone();
    relm4::spawn(async move {
        while let Some(event) = adapter_events.next().await {
            event_tx_clone
                .send(BluetoothEvent::Adapter(event))
                .unwrap_or_else(|e| log::error!("couldn't send adapter bluetooth event: {e}"));
        }
        log::error!("bluetooth service has stopped receiving adapter events");
    });

    // monitor existing devices for connection status changes
    let devices = adapter.device_addresses().await.unwrap_or_default();
    for addr in devices {
        if let Ok(device) = adapter.device(addr)
            && let Ok(mut device_events) = device.events().await
        {
            let event_tx_clone = event_tx.clone();
            relm4::spawn(async move {
                while let Some(event) = device_events.next().await {
                    event_tx_clone
                        .send(BluetoothEvent::Device(addr, event))
                        .unwrap_or_else(|e| {
                            log::error!("couldn't send device bluetooth event: {e}")
                        });
                }
                log::warn!(
                    "bluetooth service has stopped receiving events for device address {}",
                    addr
                );
            });
        }
    }

    Ok((event_tx, event_rx))
}

async fn update(input: BluetoothEvent, event_tx: &UnboundedSender<BluetoothEvent>) {
    if let BluetoothEvent::Wake = input {
        refresh_state_after_wake().await;
        return;
    }

    // update_from_event is sync so the write lock is always released before the
    // async subscription below
    let new_device = update_from_event(input);

    // subscribe to events for any newly added device; this must
    // happen outside the write lock (hence the two-step approach above)
    if let Some((address, device)) = new_device {
        subscribe_device_events(address, device, event_tx).await;
    }
}

/// Re-polls adapter powered/discovering state and connected device count after
/// a system wake, since D-Bus events may have been missed during sleep.
async fn refresh_state_after_wake() {
    log::debug!("system wake: refreshing bluetooth state");

    // clone the adapter and devices list without holding the write lock
    let (adapter, devices) = {
        let state = BLUETOOTH_STATE.read();
        let Some(ref state) = *state else {
            return;
        };
        (state.adapter.clone(), state.devices.clone())
    };

    let powered = adapter.is_powered().await.unwrap_or(false);
    let discovering = adapter.is_discovering().await.unwrap_or(false);

    let mut connected_device_count: u8 = 0;
    for device in devices.values() {
        if device.is_connected().await.unwrap_or(false) {
            connected_device_count = connected_device_count.saturating_add(1);
        }
    }

    if let Some(ref mut state) = *BLUETOOTH_STATE.write() {
        state.powered = powered;
        state.discovering = discovering;
        state.connected_device_count = connected_device_count;
    }
}

/// Applies a bluetooth event to [`BLUETOOTH_STATE`] synchronously.
///
/// Returns `Some((address, device))` when a new device was added that needs
/// an event subscription set up (handled asynchronously by the caller).
fn update_from_event(input: BluetoothEvent) -> Option<(Address, Device)> {
    let mut guard = BLUETOOTH_STATE.write();
    let state = (*guard).as_mut()?;

    log::debug!("updating bluetooth state with event: {:?}", input);

    match input {
        BluetoothEvent::Adapter(adapter_event) => match adapter_event {
            AdapterEvent::DeviceAdded(address) => {
                let Ok(device) = state.adapter.device(address) else {
                    return None;
                };
                state.devices.insert(address, device.clone());
                Some((address, device))
            }
            AdapterEvent::DeviceRemoved(address) => {
                state.devices.remove(&address);
                None
            }
            AdapterEvent::PropertyChanged(adapter_property) => {
                match adapter_property {
                    AdapterProperty::Powered(p) => state.powered = p,
                    AdapterProperty::Discovering(d) => state.discovering = d,
                    p => log::warn!("unhandled AdapterProperty event: {p:?}"),
                }
                None
            }
        },
        // wake is handled before this function is called
        BluetoothEvent::Wake => None,
        BluetoothEvent::Device(address, device_event) => {
            match device_event {
                DeviceEvent::PropertyChanged(DeviceProperty::Connected(connected)) => {
                    if connected {
                        state.connected_device_count =
                            state.connected_device_count.saturating_add(1);
                    } else {
                        state.connected_device_count =
                            state.connected_device_count.saturating_sub(1);
                    }
                    log::debug!(
                        "device {address} connected={connected}, count={}",
                        state.connected_device_count
                    );
                }
                DeviceEvent::PropertyChanged(device_property) => {
                    log::debug!("device {address} property changed: {device_property:?}");
                }
            }
            None
        }
    }
}

/// Subscribes to BlueZ property change events for a device and forwards them
/// into the shared event channel.
async fn subscribe_device_events(
    address: Address,
    device: Device,
    event_tx: &UnboundedSender<BluetoothEvent>,
) {
    match device.events().await {
        Ok(mut device_events) => {
            let tx = event_tx.clone();
            relm4::spawn(async move {
                while let Some(event) = device_events.next().await {
                    tx.send(BluetoothEvent::Device(address, event))
                        .unwrap_or_else(|e| {
                            log::error!("couldn't send device bluetooth event for {address}: {e}")
                        });
                }
                log::warn!("bluetooth event stream ended for device {address}");
            });
        }
        Err(e) => {
            log::warn!("couldn't subscribe to events for device {address}: {e}");
        }
    }
}
