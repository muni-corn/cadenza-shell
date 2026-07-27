use std::collections::{HashMap, hash_map};

use bluer::{Adapter, Address, Device, Session};
use relm4::SharedState;

pub static BLUETOOTH_STATE: SharedState<Option<BluetoothState>> = SharedState::new();

/// A point-in-time snapshot of a remote device's properties.
///
/// Devices themselves (`bluer::Device`) are cheap, synchronous handles that
/// can be reconstructed from an address at any time via
/// [`BluetoothState::device_handle`]; reading their properties, however,
/// requires an async D-Bus round trip. Snapshotting the properties we care
/// about into shared state means the UI can render device rows and
/// tooltips synchronously from cached data instead of doing D-Bus I/O on
/// every render.
#[derive(Clone, Debug)]
pub struct DeviceInfo {
    pub address: Address,
    /// The device's display name. Falls back to the remote device name if
    /// no alias has been set (BlueZ's own fallback behavior for this
    /// property).
    pub alias: String,
    pub icon: Option<String>,
    pub paired: bool,
    pub trusted: bool,
    pub connected: bool,
    /// Signal strength, if the device is currently visible in a scan.
    pub rssi: Option<i16>,
    pub battery_percentage: Option<u8>,
}

#[derive(Clone, Debug)]
pub struct BluetoothState {
    pub(super) _session: Session,
    pub adapter: Adapter,
    pub(super) devices: HashMap<Address, DeviceInfo>,
    pub discovering: bool,

    pub powered: bool,
    pub connected_device_count: u8,
}

impl BluetoothState {
    pub fn devices(&self) -> hash_map::Values<'_, Address, DeviceInfo> {
        self.devices.values()
    }

    /// Builds a handle for issuing commands (connect/disconnect/pair/etc.)
    /// against a device by address.
    ///
    /// This is a cheap, synchronous local construction (it only builds a
    /// D-Bus object path; no I/O is performed), so it's fine to call this on
    /// demand rather than caching the handle alongside the snapshot.
    pub fn device_handle(&self, address: Address) -> bluer::Result<Device> {
        self.adapter.device(address)
    }
}
