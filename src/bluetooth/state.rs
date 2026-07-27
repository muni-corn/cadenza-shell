use std::collections::{HashMap, hash_map};

use bluer::{Adapter, Address, Device, Session};
use relm4::SharedState;

pub static BLUETOOTH_STATE: SharedState<Option<BluetoothState>> = SharedState::new();

/// The pairing prompt currently awaiting a response from the user, if any.
///
/// Set by the pairing agent (`bluetooth::agent`) when BlueZ needs input or
/// confirmation from us, and cleared once answered (via
/// `agent::respond`/`agent::cancel`) or once BlueZ cancels the request.
pub static PAIRING_PROMPT: SharedState<Option<PairingPrompt>> = SharedState::new();

#[derive(Debug, Clone)]
pub struct PairingPrompt {
    pub address: Address,
    pub request: PairingRequest,
}

/// What BlueZ is asking us for, or asking us to show, during pairing.
#[derive(Debug, Clone)]
pub enum PairingRequest {
    /// The remote device needs a PIN code (legacy, pre-2.1 devices).
    PinCode,
    /// The remote device needs a numeric passkey (0-999999).
    Passkey,
    /// Confirm that this passkey matches what's displayed on the remote
    /// device.
    ConfirmPasskey(u32),
    /// Authorize the pairing with no code exchange (just-works model).
    Authorize,
    /// Display this PIN code; the user types it on the *other* device.
    DisplayPinCode(String),
    /// Display this passkey; the user types it on the *other* device.
    /// `entered` counts how many digits they've typed there so far.
    DisplayPasskey { passkey: u32, entered: u16 },
}

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
}

impl BluetoothState {
    pub fn devices(&self) -> hash_map::Values<'_, Address, DeviceInfo> {
        self.devices.values()
    }

    /// Number of currently-connected devices.
    ///
    /// Derived from the device snapshots on every read rather than tracked
    /// as a separate incrementally-updated counter, so it can never drift
    /// out of sync with the devices it's supposed to be counting (a missed
    /// or duplicated `Connected` event used to desync a standalone counter
    /// permanently until the next system wake).
    pub fn connected_device_count(&self) -> usize {
        self.devices.values().filter(|d| d.connected).count()
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
