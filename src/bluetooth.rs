use std::collections::{HashMap, hash_map};

use bluer::{Adapter, AdapterEvent, AdapterProperty, Address, Device, DeviceEvent, Session};
use futures_lite::StreamExt;
use relm4::SharedState;
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

pub static BLUETOOTH_STATE: SharedState<Option<BluetoothState>> = SharedState::new();

#[derive(Debug)]
pub enum BluetoothEvent {
    Adapter(AdapterEvent),
    Device(Address, DeviceEvent),
}

#[derive(Clone, Debug)]
pub struct BluetoothState {
    _session: Session,
    adapter: Adapter,
    devices: HashMap<Address, Device>,
    discovering: bool,

    pub powered: bool,
    pub connected_device_count: u8,
}

impl BluetoothState {
    pub fn devices(&self) -> hash_map::Values<'_, Address, Device> {
        self.devices.values()
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

    let state = BluetoothState {
        _session: session,
        powered: adapter.is_powered().await.unwrap_or(false),
        connected_device_count: 0,
        devices,
        discovering: adapter.is_discovering().await.unwrap_or(false),
        adapter: adapter.clone(),
    };

    *BLUETOOTH_STATE.write() = Some(state);

    // set up bluetooth monitoring
    match start_event_listening(adapter).await {
        Ok(mut event_rx) => {
            while let Some(event) = event_rx.recv().await {
                update(event).await;
            }
        }
        Err(e) => {
            log::error!("failed to setup bluetooth monitoring: {}", e);
        }
    }
}

async fn start_event_listening(
    adapter: Adapter,
) -> anyhow::Result<UnboundedReceiver<BluetoothEvent>> {
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

    // start device discovery
    let mut discovery_events = adapter.discover_devices().await?;
    let event_tx_clone = event_tx.clone();
    relm4::spawn(async move {
        while let Some(event) = discovery_events.next().await {
            event_tx_clone
                .send(BluetoothEvent::Adapter(event))
                .unwrap_or_else(|e| log::error!("couldn't send discovery bluetooth event: {e}"));
        }
        log::error!("bluetooth service has stopped receiving discovery events");
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

    Ok(event_rx)
}

async fn update(input: BluetoothEvent) {
    update_from_event(input).await;
    update_connected_device_count().await;
}

async fn update_from_event(input: BluetoothEvent) {
    let Some(ref mut state) = *BLUETOOTH_STATE.write() else {
        return;
    };

    log::debug!("updating bluetooth state with event: {:?}", input);

    match input {
        BluetoothEvent::Adapter(adapter_event) => match adapter_event {
            AdapterEvent::DeviceAdded(address) => {
                let Ok(device) = state.adapter.device(address) else {
                    return;
                };
                state.devices.insert(address, device);
            }
            AdapterEvent::DeviceRemoved(address) => {
                state.devices.remove(&address);
            }
            AdapterEvent::PropertyChanged(adapter_property) => match adapter_property {
                AdapterProperty::Powered(p) => {
                    state.powered = p;
                }
                AdapterProperty::Discovering(d) => {
                    state.discovering = d;
                }
                p => log::warn!("unhandled AdapterProperty event: {p:?}"),
            },
        },
        BluetoothEvent::Device(address, device_event) => match device_event {
            DeviceEvent::PropertyChanged(device_property) => {
                log::warn!(
                    "unhandled ProperyChanged event for device {address}: {device_property:?}"
                )
            }
        },
    }
}

async fn update_connected_device_count() {
    log::debug!("updating connected device count for bluetooth");

    let devices = {
        let state = BLUETOOTH_STATE.read();
        let Some(ref state) = *state else {
            return;
        };
        state.devices.clone()
    };

    let mut count = 0;
    for device in devices.values() {
        if device.is_connected().await.unwrap_or(false) {
            count += 1;
        }
    }

    if let Some(ref mut state) = *BLUETOOTH_STATE.write() {
        state.connected_device_count = count;
    }
}
