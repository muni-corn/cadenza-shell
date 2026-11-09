//! Applying property changes to the shared state.

use bluer::{AdapterProperty, Address, DeviceProperty};

use crate::{
    bluetooth::state::{BLUETOOTH_STATE, BluetoothState},
    utils::shared_state::set_if_changed,
};

/// Applies an adapter property change to [`BLUETOOTH_STATE`].
pub(super) fn apply_adapter_property(property: AdapterProperty) {
    // other properties (class, uuids, discoverable, ...) aren't shown, so
    // don't wake every subscriber for them
    if !matches!(
        property,
        AdapterProperty::Powered(_) | AdapterProperty::Discovering(_)
    ) {
        tracing::trace!("ignoring adapter property change: {property:?}");
        return;
    }

    update_state(|state| match property {
        AdapterProperty::Powered(powered) => state.powered = powered,
        AdapterProperty::Discovering(discovering) => state.discovering = discovering,
        _ => {}
    });
}

/// Applies a device property change to [`BLUETOOTH_STATE`].
pub(super) fn apply_device_property(address: Address, property: DeviceProperty) {
    // properties the shell doesn't show (rssi changes several times a second
    // during discovery) would otherwise wake every subscriber for nothing
    if !matches!(
        property,
        DeviceProperty::Connected(_)
            | DeviceProperty::Alias(_)
            | DeviceProperty::Icon(_)
            | DeviceProperty::Paired(_)
    ) {
        tracing::trace!("ignoring device {address} property change: {property:?}");
        return;
    }

    update_state(|state| {
        let Some(info) = state.devices.get_mut(&address) else {
            tracing::debug!("property change for untracked device {address}, ignoring");
            return;
        };
        match property {
            DeviceProperty::Connected(connected) => info.connected = connected,
            DeviceProperty::Alias(alias) => info.alias = alias,
            DeviceProperty::Icon(icon) => info.icon = Some(icon),
            DeviceProperty::Paired(paired) => info.paired = paired,
            _ => {}
        }
    });
}

/// Applies `update` to a copy of the current state and publishes it only if
/// something changed, since every write wakes every tile and menu.
pub(super) fn update_state(update: impl FnOnce(&mut BluetoothState)) {
    let Some(mut state) = BLUETOOTH_STATE.read().clone() else {
        return;
    };
    update(&mut state);
    set_if_changed(&BLUETOOTH_STATE, Some(state));
}
