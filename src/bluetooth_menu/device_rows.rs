//! The menu's paired and nearby device lists.

use std::collections::{HashMap, HashSet};

use bluer::Address;
use relm4::{Sender, prelude::*};

use super::BluetoothMenuMsg;
use crate::{
    bluetooth::{ActionProgress, BluetoothState, DeviceAction, DeviceInfo},
    widgets::menu_row::{MenuRow, MenuRowData, MenuRowOutput, sync_rows},
};

/// The rows for paired devices and for nearby unpaired ones.
#[derive(Debug)]
pub struct DeviceRows {
    paired: FactoryVecDeque<MenuRow>,
    nearby: FactoryVecDeque<MenuRow>,
    /// Why the last action on each device failed, until the next one starts.
    errors: HashMap<Address, String>,
    /// Devices with an action in flight.
    busy: HashSet<Address>,
}

impl DeviceRows {
    /// Creates empty lists whose clicks go to the menu through `menu`.
    pub fn new(menu: &Sender<BluetoothMenuMsg>) -> Self {
        Self {
            paired: device_list(menu),
            nearby: device_list(menu),
            errors: HashMap::new(),
            busy: HashSet::new(),
        }
    }

    /// Records how an action on the device at `address` is going; call
    /// [`Self::sync`] afterwards to show it.
    pub fn apply_progress(
        &mut self,
        address: Address,
        action: DeviceAction,
        progress: &ActionProgress,
    ) {
        match progress {
            ActionProgress::Started => {
                self.busy.insert(address);
                self.errors.remove(&address);
            }
            ActionProgress::Succeeded => {
                self.busy.remove(&address);
            }
            ActionProgress::Failed(reason) => {
                self.busy.remove(&address);
                self.errors.insert(address, failure_text(action, reason));
            }
        }
    }

    pub fn paired_widget(&self) -> &gtk::Box {
        self.paired.widget()
    }

    pub fn nearby_widget(&self) -> &gtk::Box {
        self.nearby.widget()
    }

    /// Forgets which devices have actions in flight; call [`Self::sync`]
    /// afterwards to show it.
    pub fn clear_busy(&mut self) {
        self.busy.clear();
    }

    /// Makes the lists show the devices in `state`.
    pub fn sync(&mut self, state: Option<&BluetoothState>) {
        let devices = || state.into_iter().flat_map(BluetoothState::devices);
        let paired = self.rows_for(devices().filter(|d| d.paired));
        let nearby = self.rows_for(devices().filter(|d| !d.paired));
        sync_rows(&mut self.paired, paired);
        sync_rows(&mut self.nearby, nearby);
    }

    /// Rows for `devices`, connected devices first and then by name; devices
    /// come out of a hashmap, so they need a stable order.
    fn rows_for<'a>(&self, devices: impl Iterator<Item = &'a DeviceInfo>) -> Vec<MenuRowData> {
        let mut devices: Vec<&DeviceInfo> = devices.collect();
        devices.sort_by(|a, b| {
            b.connected
                .cmp(&a.connected)
                .then_with(|| a.alias.cmp(&b.alias))
                .then_with(|| a.address.cmp(&b.address))
        });
        devices
            .into_iter()
            .map(|info| MenuRowData {
                key: info.address.to_string(),
                icon: info.icon.clone(),
                title: info.alias.clone(),
                status: info.connected.then_some("Connected"),
                remove_tooltip: info.paired.then_some("Remove this device"),
                error: self.errors.get(&info.address).cloned(),
                busy: self.busy.contains(&info.address),
            })
            .collect()
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

/// Tells the user which action failed and why.
fn failure_text(action: DeviceAction, reason: &str) -> String {
    let action = match action {
        DeviceAction::Connect => "connect",
        DeviceAction::Disconnect => "disconnect",
        DeviceAction::Pair => "pair",
        DeviceAction::Remove => "remove it",
    };
    format!("Couldn't {action}: {reason}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failures_name_the_action() {
        assert_eq!(
            failure_text(DeviceAction::Remove, "the device is gone"),
            "Couldn't remove it: the device is gone"
        );
    }
}
