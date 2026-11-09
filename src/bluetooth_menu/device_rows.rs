//! The menu's paired and nearby device lists.

use bluer::Address;
use relm4::{Sender, prelude::*};

use super::BluetoothMenuMsg;
use crate::{
    bluetooth::{BluetoothState, DeviceInfo},
    widgets::menu_row::{MenuRow, MenuRowData, MenuRowOutput, sync_rows},
};

/// The rows for paired devices and for nearby unpaired ones.
#[derive(Debug)]
pub struct DeviceRows {
    paired: FactoryVecDeque<MenuRow>,
    nearby: FactoryVecDeque<MenuRow>,
}

impl DeviceRows {
    /// Creates empty lists whose clicks go to the menu through `menu`.
    pub fn new(menu: &Sender<BluetoothMenuMsg>) -> Self {
        Self {
            paired: device_list(menu),
            nearby: device_list(menu),
        }
    }

    pub fn paired_widget(&self) -> &gtk::Box {
        self.paired.widget()
    }

    pub fn nearby_widget(&self) -> &gtk::Box {
        self.nearby.widget()
    }

    /// Makes the lists show the devices in `state`.
    pub fn sync(&mut self, state: Option<&BluetoothState>) {
        let devices = || state.into_iter().flat_map(BluetoothState::devices);
        sync_rows(
            &mut self.paired,
            device_rows(devices().filter(|d| d.paired)),
        );
        sync_rows(
            &mut self.nearby,
            device_rows(devices().filter(|d| !d.paired)),
        );
    }
}

fn device_list(menu: &Sender<BluetoothMenuMsg>) -> FactoryVecDeque<MenuRow> {
    FactoryVecDeque::builder()
        .launch(
            gtk::Box::builder()
                .orientation(gtk::Orientation::Vertical)
                .spacing(8)
                .build(),
        )
        .forward(menu, device_output_to_msg)
}

fn device_output_to_msg(output: MenuRowOutput) -> BluetoothMenuMsg {
    let (MenuRowOutput::Activated(key) | MenuRowOutput::Remove(key)) = &output;
    // rows are keyed by the address they were built from, so this can't fail
    let address: Address = key.parse().expect("device rows are keyed by address");
    match output {
        MenuRowOutput::Activated(_) => BluetoothMenuMsg::DeviceClicked(address),
        MenuRowOutput::Remove(_) => BluetoothMenuMsg::RemoveDevice(address),
    }
}

/// Rows for `devices`, connected devices first and then by name; devices
/// come out of a hashmap, so they need a stable order.
fn device_rows<'a>(devices: impl Iterator<Item = &'a DeviceInfo>) -> Vec<MenuRowData> {
    let mut devices: Vec<&DeviceInfo> = devices.collect();
    devices.sort_by(|a, b| {
        b.connected
            .cmp(&a.connected)
            .then_with(|| a.alias.cmp(&b.alias))
            .then_with(|| a.address.cmp(&b.address))
    });
    devices.into_iter().map(device_row).collect()
}

/// How a device is shown in the list, keyed by its address.
fn device_row(info: &DeviceInfo) -> MenuRowData {
    MenuRowData {
        key: info.address.to_string(),
        icon: info.icon.clone(),
        title: info.alias.clone(),
        status: info.connected.then_some("Connected"),
        remove_tooltip: info.paired.then_some("Remove this device"),
    }
}
