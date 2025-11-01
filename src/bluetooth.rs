use std::collections::HashMap;

use bluer::{
    Adapter, AdapterEvent, AdapterProperty, Address, Device, DeviceEvent, DeviceProperty, Session,
};
use futures_lite::StreamExt;
use relm4::{AsyncReducer, AsyncReducible};

pub static BLUETOOTH_REDUCER: AsyncReducer<BluetoothReducible> = AsyncReducer::new();

#[derive(Debug)]
pub enum BluetoothEvent {
    Adapter(AdapterEvent),
    Device(Address, DeviceEvent),
}

pub enum BluetoothReducible {
    Unavailable,
    Available {
        _session: Session,
        adapter: Adapter,
        state: BluetoothState,
    },
}

impl BluetoothReducible {
    pub fn state(&self) -> Option<&BluetoothState> {
        if let Self::Available { state, .. } = self {
            Some(state)
        } else {
            None
        }
    }
}

impl AsyncReducible for BluetoothReducible {
    type Input = BluetoothEvent;

    async fn init() -> Self {
        let Ok(session) = Session::new()
            .await
            .inspect_err(|e| log::error!("couldn't initialize bluetooth session: {e}"))
        else {
            return Self::Unavailable;
        };

        let Ok(adapter) = session
            .default_adapter()
            .await
            .inspect_err(|e| log::error!("couldn't get default bluetooth adapter: {e}"))
        else {
            return Self::Unavailable;
        };

        relm4::spawn(start_bluetooth_watcher(adapter.clone()));

        let mut devices = HashMap::new();
        if let Ok(addresses) = adapter.device_addresses().await {
            for address in addresses {
                let Ok(device) = adapter.device(address) else {
                    continue;
                };
                devices.insert(address, device);
            }
        };

        let mut state = BluetoothState {
            powered: adapter.is_powered().await.unwrap_or(false),
            connected_device_count: 0,
            devices,
            discovering: adapter.is_discovering().await.unwrap_or(false),
        };
        state.update_connected_device_count().await;

        Self::Available {
            _session: session,
            adapter,
            state,
        }
    }

    async fn reduce(&mut self, input: Self::Input) -> bool {
        log::debug!("reducing with bluetooth event: {:?}", input);
        let Self::Available { state, adapter, .. } = self else {
            return false;
        };

        match input {
            BluetoothEvent::Adapter(adapter_event) => match adapter_event {
                AdapterEvent::DeviceAdded(address) => {
                    let Ok(device) = adapter.device(address) else {
                        return false;
                    };
                    state.devices.insert(address, device);
                    state.update_connected_device_count().await;
                    true
                }
                AdapterEvent::DeviceRemoved(address) => {
                    state.devices.remove(&address);
                    state.update_connected_device_count().await;
                    true
                }
                AdapterEvent::PropertyChanged(adapter_property) => match adapter_property {
                    AdapterProperty::Powered(powered) => {
                        state.powered = powered;
                        true
                    }
                    AdapterProperty::Discovering(discovering) => {
                        state.discovering = discovering;
                        true
                    }
                    AdapterProperty::Address(_)
                    | AdapterProperty::AddressType(_)
                    | AdapterProperty::SystemName(_)
                    | AdapterProperty::Alias(_)
                    | AdapterProperty::Class(_)
                    | AdapterProperty::Discoverable(_)
                    | AdapterProperty::Pairable(_)
                    | AdapterProperty::PairableTimeout(_)
                    | AdapterProperty::DiscoverableTimeout(_)
                    | AdapterProperty::Uuids(_)
                    | AdapterProperty::Modalias(_)
                    | AdapterProperty::ActiveAdvertisingInstances(_)
                    | AdapterProperty::SupportedAdvertisingInstances(_)
                    | AdapterProperty::SupportedAdvertisingSystemIncludes(_)
                    | AdapterProperty::SupportedAdvertisingSecondaryChannels(_)
                    | AdapterProperty::SupportedAdvertisingCapabilities(_)
                    | AdapterProperty::SupportedAdvertisingFeatures(_)
                    | _ => false,
                },
            },
            BluetoothEvent::Device(address, device_event) => match device_event {
                DeviceEvent::PropertyChanged(device_property) => match device_property {
                    DeviceProperty::Connected(_connected) => {
                        if let Some(_device) = state.devices.get(&address) {
                            // update our count
                            state.update_connected_device_count().await;
                            true
                        } else {
                            false
                        }
                    }
                    DeviceProperty::Name(_)
                    | DeviceProperty::RemoteAddress(_)
                    | DeviceProperty::AddressType(_)
                    | DeviceProperty::Icon(_)
                    | DeviceProperty::Class(_)
                    | DeviceProperty::Appearance(_)
                    | DeviceProperty::Uuids(_)
                    | DeviceProperty::Paired(_)
                    | DeviceProperty::Trusted(_)
                    | DeviceProperty::Blocked(_)
                    | DeviceProperty::WakeAllowed(_)
                    | DeviceProperty::Alias(_)
                    | DeviceProperty::LegacyPairing(_)
                    | DeviceProperty::Modalias(_)
                    | DeviceProperty::Rssi(_)
                    | DeviceProperty::TxPower(_)
                    | DeviceProperty::ManufacturerData(_)
                    | DeviceProperty::ServiceData(_)
                    | DeviceProperty::ServicesResolved(_)
                    | DeviceProperty::AdvertisingFlags(_)
                    | DeviceProperty::AdvertisingData(_)
                    | DeviceProperty::BatteryPercentage(_)
                    | _ => false,
                },
            },
        }
    }
}

#[derive(Debug, Clone)]
pub struct BluetoothState {
    pub powered: bool,
    pub devices: HashMap<Address, Device>,
    pub connected_device_count: u8,
    pub discovering: bool,
}

impl BluetoothState {
    async fn update_connected_device_count(&mut self) {
        let mut count = 0;
        for device in self.devices.values() {
            if device.is_connected().await.unwrap_or(false) {
                count += 1;
            }
        }
        self.connected_device_count = count;
    }
}

async fn start_bluetooth_watcher(adapter: Adapter) {
    // set up bluetooth monitoring
    if let Err(e) = start_loop(adapter).await {
        log::error!("failed to setup bluetooth monitoring: {}", e);
    }
}

async fn start_loop(adapter: Adapter) -> anyhow::Result<()> {
    // monitor adapter events
    let mut adapter_events = adapter.events().await?;
    relm4::spawn(async move {
        while let Some(event) = adapter_events.next().await {
            BLUETOOTH_REDUCER.emit(BluetoothEvent::Adapter(event));
        }
    });

    // start device discovery
    let mut discovery_events = adapter.discover_devices().await?;
    relm4::spawn(async move {
        while let Some(event) = discovery_events.next().await {
            BLUETOOTH_REDUCER.emit(BluetoothEvent::Adapter(event));
        }
    });

    // Monitor existing devices for connection status changes
    let devices = adapter.device_addresses().await.unwrap_or_default();
    for addr in devices {
        if let Ok(device) = adapter.device(addr)
            && let Ok(mut device_events) = device.events().await
        {
            relm4::spawn(async move {
                while let Some(event) = device_events.next().await {
                    BLUETOOTH_REDUCER.emit(BluetoothEvent::Device(addr, event));
                }
            });
        }
    }

    Ok(())
}
