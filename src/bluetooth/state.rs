use std::collections::{HashMap, hash_map};

use bluer::{Adapter, Address, Device, Session};
use relm4::SharedState;

pub static BLUETOOTH_STATE: SharedState<Option<BluetoothState>> = SharedState::new();

#[derive(Clone, Debug)]
pub struct BluetoothState {
    pub(super) _session: Session,
    pub adapter: Adapter,
    pub(super) devices: HashMap<Address, Device>,
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
