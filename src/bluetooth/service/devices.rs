//! Tracking the adapter and its devices.

use std::collections::HashMap;

use bluer::{Address, Device};
use futures_lite::StreamExt;

use super::{BluetoothEvent, BluetoothService, apply::update_state};
use crate::{
    bluetooth::state::{BLUETOOTH_STATE, BluetoothState, DeviceInfo},
    utils::shared_state::set_if_changed,
};

impl BluetoothService {
    /// (Re-)establishes [`BLUETOOTH_STATE`] and its watcher tasks against the
    /// session's current default adapter, or clears state to `None` if none
    /// is currently available.
    ///
    /// Called at startup and whenever the tracked adapter goes away, since a
    /// restarted bluez or a replugged adapter needs fresh subscriptions.
    pub(super) async fn reinitialize_adapter(&mut self) {
        self.stop_adapter_tasks();

        let adapter = match self.session.default_adapter().await {
            Ok(adapter) => adapter,
            Err(e) => {
                tracing::warn!("no bluetooth adapter currently available: {e}");
                set_if_changed(&BLUETOOTH_STATE, None);
                return;
            }
        };

        // subscribe before snapshotting so devices and changes that appear in
        // between are delivered as events instead of being lost
        match adapter.events().await {
            Ok(mut adapter_events) => {
                let event_tx = self.event_tx.clone();
                let handle = relm4::spawn(async move {
                    while let Some(event) = adapter_events.next().await {
                        if event_tx.send(BluetoothEvent::Adapter(event)).is_err() {
                            break;
                        }
                    }
                    tracing::warn!("bluetooth service has stopped receiving adapter events");
                });
                self.adapter_events_task = Some(handle.abort_handle());
            }
            Err(e) => tracing::error!("couldn't subscribe to bluetooth adapter events: {e}"),
        }

        let mut devices = HashMap::new();
        for address in adapter.device_addresses().await.unwrap_or_default() {
            let Ok(device) = adapter.device(address) else {
                continue;
            };
            self.subscribe_device(address, device.clone()).await;
            match build_device_info(&device).await {
                Ok(info) => {
                    devices.insert(address, info);
                }
                Err(e) => tracing::debug!("couldn't read bluetooth device {address}: {e}"),
            }
        }

        let state = BluetoothState {
            powered: adapter.is_powered().await.unwrap_or(false),
            devices,
            discovering: adapter.is_discovering().await.unwrap_or(false),
        };

        self.adapter = Some(adapter);
        set_if_changed(&BLUETOOTH_STATE, Some(state));
    }

    /// Fetches a full property snapshot for a newly discovered device and
    /// inserts it into [`BLUETOOTH_STATE`].
    pub(super) async fn add_device(&mut self, address: Address) {
        let Some(adapter) = self.adapter.clone() else {
            return;
        };
        let Ok(device) = adapter.device(address) else {
            return;
        };

        // subscribe before fetching so a change in between isn't lost
        self.subscribe_device(address, device.clone()).await;

        match build_device_info(&device).await {
            Ok(info) => update_state(|state| {
                state.devices.insert(address, info);
            }),
            Err(e) => tracing::debug!("couldn't read new bluetooth device {address}: {e}"),
        }
    }

    /// Re-polls adapter state and rebuilds the device list from the
    /// adapter's own.
    ///
    /// The adapter's device list is the source of truth: devices it no
    /// longer knows are dropped along with their subscriptions, devices we
    /// never heard about are subscribed, and devices that can't be read are
    /// skipped.
    pub(super) async fn reconcile_devices(&mut self) {
        let Some(adapter) = self.adapter.clone() else {
            return;
        };

        let addresses = match adapter.device_addresses().await {
            Ok(addresses) => addresses,
            Err(e) => {
                tracing::warn!("couldn't list bluetooth devices: {e}");
                return;
            }
        };

        self.device_event_tasks.retain(|address, handle| {
            let known = addresses.contains(address);
            if !known {
                handle.abort();
            }
            known
        });

        let mut devices = HashMap::with_capacity(addresses.len());
        for address in addresses {
            let Ok(device) = adapter.device(address) else {
                continue;
            };
            if !self.device_event_tasks.contains_key(&address) {
                self.subscribe_device(address, device.clone()).await;
            }
            match build_device_info(&device).await {
                Ok(info) => {
                    devices.insert(address, info);
                }
                Err(e) => tracing::debug!("couldn't read bluetooth device {address}: {e}"),
            }
        }

        let powered = adapter.is_powered().await.unwrap_or(false);
        let discovering = adapter.is_discovering().await.unwrap_or(false);

        update_state(|state| {
            state.powered = powered;
            state.discovering = discovering;
            state.devices = devices;
        });
    }

    /// Forwards a device's property changes into the event channel,
    /// replacing any stale subscription for the same address (e.g. it was
    /// removed and re-added) rather than leaking it.
    pub(super) async fn subscribe_device(&mut self, address: Address, device: Device) {
        let mut device_events = match device.events().await {
            Ok(events) => events,
            Err(e) => {
                tracing::warn!("couldn't subscribe to events for device {address}: {e}");
                return;
            }
        };

        let event_tx = self.event_tx.clone();
        let handle = relm4::spawn(async move {
            while let Some(event) = device_events.next().await {
                if event_tx
                    .send(BluetoothEvent::Device(address, event))
                    .is_err()
                {
                    break;
                }
            }
            tracing::debug!("bluetooth event stream ended for device {address}");
        })
        .abort_handle();

        if let Some(old_handle) = self.device_event_tasks.insert(address, handle) {
            old_handle.abort();
        }
    }
}

/// Fetches a full property snapshot for a device.
///
/// Fails if the device can't be read (usually because bluez forgot it), so
/// callers can drop it instead of showing made-up defaults.
async fn build_device_info(device: &Device) -> bluer::Result<DeviceInfo> {
    Ok(DeviceInfo {
        address: device.address(),
        alias: device.alias().await?,
        icon: device.icon().await?,
        paired: device.is_paired().await?,
        connected: device.is_connected().await?,
    })
}
