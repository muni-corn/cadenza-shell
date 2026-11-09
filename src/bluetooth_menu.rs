mod pairing_prompt;

use bluer::Address;
use gtk4::prelude::*;
use relm4::prelude::*;

use self::pairing_prompt::{PromptView, parse_passkey, parse_pin_code, prompt_view};
use crate::{
    bluetooth::{
        self, BLUETOOTH_STATE, BluetoothState, DeviceInfo, PAIRING_PROMPT, PairingPrompt,
        PairingReply, PairingRequest, get_icon,
    },
    widgets::{
        menu_parts::{MenuHeader, PromptBox, menu_root, scrolled_content},
        menu_row::{MenuRow, MenuRowData, MenuRowOutput, sync_rows},
    },
};

#[derive(Debug)]
pub struct BluetoothMenu {
    bluetooth_state: Option<BluetoothState>,
    paired_devices: FactoryVecDeque<MenuRow>,
    nearby_devices: FactoryVecDeque<MenuRow>,
    pairing_prompt: Option<PairingPrompt>,
    /// Why the last PIN or passkey entry was rejected, if it was.
    pairing_error: Option<&'static str>,
    /// Set when a new prompt arrives, so the entry is cleared and focused.
    reset_pairing_entry: bool,
    /// Set when new state arrives, so the switch only snaps to the real
    /// value then and not right after the user flips it.
    resync_power_switch: bool,
}

#[derive(Debug)]
pub enum BluetoothMenuMsg {
    ToggleBluetooth(bool),
    ToggleDiscovery,
    /// A device's row was clicked.
    DeviceClicked(Address),
    RemoveDevice(Address),
    UpdateState(Option<BluetoothState>),
    UpdatePairingPrompt(Option<PairingPrompt>),
    PairingSubmit(String),
    PairingConfirm,
    PairingCancel,
    /// Sent by the owning tile when the menu window opens or closes.
    SetOpen(bool),
}

#[derive(Debug)]
pub struct BluetoothMenuWidgets {
    header: MenuHeader,
    status_label: gtk::Label,
    scan_button: gtk::Button,
    pairing_prompt: PromptBox,
}

impl SimpleComponent for BluetoothMenu {
    type Init = ();
    type Input = BluetoothMenuMsg;
    type Output = ();
    type Root = gtk::Box;
    type Widgets = BluetoothMenuWidgets;

    fn init(
        _init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        // subscribe before reading the initial state below, so a write in
        // between isn't missed
        BLUETOOTH_STATE.subscribe(sender.input_sender(), |state| {
            BluetoothMenuMsg::UpdateState(state.to_owned())
        });
        PAIRING_PROMPT.subscribe(sender.input_sender(), |prompt| {
            BluetoothMenuMsg::UpdatePairingPrompt(prompt.to_owned())
        });

        // create device factories, one per section
        let paired_devices = FactoryVecDeque::builder()
            .launch(
                gtk::Box::builder()
                    .orientation(gtk::Orientation::Vertical)
                    .spacing(8)
                    .build(),
            )
            .forward(sender.input_sender(), device_output_to_msg);
        let nearby_devices = FactoryVecDeque::builder()
            .launch(
                gtk::Box::builder()
                    .orientation(gtk::Orientation::Vertical)
                    .spacing(8)
                    .build(),
            )
            .forward(sender.input_sender(), device_output_to_msg);

        let model = BluetoothMenu {
            bluetooth_state: BLUETOOTH_STATE.read().clone(),
            paired_devices,
            nearby_devices,
            pairing_prompt: PAIRING_PROMPT.read().clone(),
            pairing_error: None,
            reset_pairing_entry: true,
            resync_power_switch: false,
        };

        let header = MenuHeader::new(
            get_icon(model.bluetooth_state.as_ref()),
            model.bluetooth_state.as_ref().is_some_and(|s| s.powered),
            {
                let sender = sender.clone();
                move |powered| sender.input(BluetoothMenuMsg::ToggleBluetooth(powered))
            },
        );
        root.append(&header.root);

        let (scrolled_window, content_box) = scrolled_content();

        // status + scan row
        let status_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(4)
            .build();

        let status_label = gtk::Label::builder()
            .halign(gtk::Align::Start)
            .label(get_status_text(&model.bluetooth_state))
            .build();

        let scan_button = gtk::Button::builder()
            .label(scan_button_label(&model.bluetooth_state))
            .halign(gtk::Align::Start)
            .build();
        scan_button.connect_clicked({
            let sender = sender.clone();
            move |_| sender.input(BluetoothMenuMsg::ToggleDiscovery)
        });

        status_box.append(&status_label);
        status_box.append(&scan_button);

        // the entry is only shown for PinCode/Passkey requests; for
        // ConfirmPasskey/Authorize there's nothing to submit, just a yes/no
        let pairing_prompt = PromptBox::new(
            "Confirm",
            "PIN or passkey",
            false,
            {
                let sender = sender.clone();
                move |text| {
                    sender.input(match text {
                        Some(text) => BluetoothMenuMsg::PairingSubmit(text),
                        None => BluetoothMenuMsg::PairingConfirm,
                    });
                }
            },
            {
                let sender = sender.clone();
                move || sender.input(BluetoothMenuMsg::PairingCancel)
            },
        );

        // paired devices section
        let paired_header = gtk::Label::builder()
            .halign(gtk::Align::Start)
            .css_classes(["section-title"])
            .label("Paired devices")
            .build();

        // nearby devices section
        let nearby_header = gtk::Label::builder()
            .halign(gtk::Align::Start)
            .css_classes(["section-title"])
            .label("Nearby devices")
            .build();

        content_box.append(&status_box);
        content_box.append(&pairing_prompt.root);
        content_box.append(&paired_header);
        content_box.append(model.paired_devices.widget());
        content_box.append(&nearby_header);
        content_box.append(model.nearby_devices.widget());
        root.append(&scrolled_window);

        let mut widgets = BluetoothMenuWidgets {
            header,
            status_label,
            scan_button,
            pairing_prompt,
        };

        let mut model = model;
        update_device_rows(
            &mut model.paired_devices,
            paired_devices_iter(&model.bluetooth_state),
        );
        update_device_rows(
            &mut model.nearby_devices,
            nearby_devices_iter(&model.bluetooth_state),
        );
        render_pairing_prompt(&model, &mut widgets);

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        self.resync_power_switch = false;
        self.reset_pairing_entry = false;
        match msg {
            BluetoothMenuMsg::UpdateState(state) => {
                self.resync_power_switch = true;
                update_device_rows(&mut self.paired_devices, paired_devices_iter(&state));
                update_device_rows(&mut self.nearby_devices, nearby_devices_iter(&state));
                self.bluetooth_state = state;
            }
            BluetoothMenuMsg::UpdatePairingPrompt(prompt) => {
                self.pairing_prompt = prompt;
                self.pairing_error = None;
                self.reset_pairing_entry = true;
            }
            BluetoothMenuMsg::ToggleBluetooth(enabled) => {
                if self
                    .bluetooth_state
                    .as_ref()
                    .is_some_and(|s| s.powered == enabled)
                {
                    return;
                }

                bluetooth::set_powered(enabled);
            }
            BluetoothMenuMsg::ToggleDiscovery => {
                let discovering = self.bluetooth_state.as_ref().is_some_and(|s| s.discovering);
                if discovering {
                    bluetooth::stop_discovery();
                } else {
                    bluetooth::start_discovery();
                }
            }
            BluetoothMenuMsg::DeviceClicked(addr) => {
                let device = self
                    .bluetooth_state
                    .iter()
                    .flat_map(|state| state.devices())
                    .find(|device| device.address == addr);
                match device {
                    Some(device) if !device.paired => bluetooth::pair(addr),
                    Some(device) if device.connected => bluetooth::disconnect(addr),
                    Some(_) => bluetooth::connect(addr),
                    None => {}
                }
            }
            BluetoothMenuMsg::RemoveDevice(addr) => {
                bluetooth::remove(addr);
            }
            BluetoothMenuMsg::PairingSubmit(text) => {
                let Some(prompt) = &self.pairing_prompt else {
                    return;
                };
                let reply = match prompt.request {
                    PairingRequest::PinCode => parse_pin_code(&text).map(PairingReply::Text),
                    PairingRequest::Passkey => parse_passkey(&text).map(PairingReply::Number),
                    // the entry isn't shown for these; nothing to submit
                    PairingRequest::ConfirmPasskey(_)
                    | PairingRequest::Authorize
                    | PairingRequest::DisplayPinCode(_)
                    | PairingRequest::DisplayPasskey { .. } => return,
                };

                match reply {
                    Ok(reply) => {
                        self.pairing_error = None;
                        bluetooth::pairing_reply(reply);
                    }
                    Err(problem) => self.pairing_error = Some(problem),
                }
            }
            // scanning only matters while someone can see the results
            BluetoothMenuMsg::SetOpen(false) => bluetooth::stop_discovery(),
            BluetoothMenuMsg::SetOpen(true) => {}
            BluetoothMenuMsg::PairingConfirm => {
                bluetooth::pairing_reply(PairingReply::Confirm);
            }
            BluetoothMenuMsg::PairingCancel => {
                bluetooth::cancel_pairing();
            }
        }
    }

    fn update_view(&self, widgets: &mut Self::Widgets, _sender: ComponentSender<Self>) {
        widgets
            .header
            .set_icon(get_icon(self.bluetooth_state.as_ref()));

        let is_powered = self.bluetooth_state.as_ref().map_or_default(|s| s.powered);
        if self.resync_power_switch {
            widgets.header.sync_switch(is_powered);
        }

        widgets
            .status_label
            .set_label(&get_status_text(&self.bluetooth_state));

        widgets.scan_button.set_sensitive(is_powered);
        widgets
            .scan_button
            .set_label(&scan_button_label(&self.bluetooth_state));

        render_pairing_prompt(self, widgets);
    }

    fn init_root() -> Self::Root {
        menu_root()
    }
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

fn paired_devices_iter(state: &Option<BluetoothState>) -> impl Iterator<Item = &DeviceInfo> {
    state.iter().flat_map(|s| s.devices()).filter(|d| d.paired)
}

fn nearby_devices_iter(state: &Option<BluetoothState>) -> impl Iterator<Item = &DeviceInfo> {
    state.iter().flat_map(|s| s.devices()).filter(|d| !d.paired)
}

fn scan_button_label(state: &Option<BluetoothState>) -> String {
    if state.as_ref().is_some_and(|s| s.discovering) {
        "Stop scanning".to_string()
    } else {
        "Scan for devices".to_string()
    }
}

fn render_pairing_prompt(model: &BluetoothMenu, widgets: &mut BluetoothMenuWidgets) {
    let Some(prompt) = &model.pairing_prompt else {
        widgets.pairing_prompt.hide();
        return;
    };

    let device = device_name(&model.bluetooth_state, prompt.address);
    let PromptView {
        text,
        show_entry,
        show_buttons,
    } = prompt_view(&prompt.request, &device);

    widgets.pairing_prompt.show(&text, show_entry, show_buttons);
    widgets.pairing_prompt.set_error(model.pairing_error);
    if model.reset_pairing_entry {
        widgets.pairing_prompt.reset_entry();
    }
}

/// The device's alias if it's known, otherwise its address.
fn device_name(state: &Option<BluetoothState>, address: Address) -> String {
    state
        .iter()
        .flat_map(|state| state.devices())
        .find(|device| device.address == address)
        .map_or_else(|| address.to_string(), |device| device.alias.clone())
}

fn get_status_text(state: &Option<BluetoothState>) -> String {
    match state {
        Some(s) if !s.powered => "Bluetooth disabled".to_string(),
        Some(s) if s.discovering => "Searching for devices...".to_string(),
        Some(s) if s.connected_device_count() == 1 => "1 device connected".to_string(),
        Some(s) if s.connected_device_count() > 0 => {
            format!("{} devices connected", s.connected_device_count())
        }
        Some(_) => "Bluetooth enabled".to_string(),
        None => "Bluetooth unavailable".to_string(),
    }
}

/// Syncs device rows with the latest snapshots, connected devices first and
/// then by name; devices come out of a hashmap, so they need a stable order.
fn update_device_rows<'a>(
    rows: &mut FactoryVecDeque<MenuRow>,
    infos: impl Iterator<Item = &'a DeviceInfo>,
) {
    let mut infos: Vec<&DeviceInfo> = infos.collect();
    infos.sort_by(|a, b| {
        b.connected
            .cmp(&a.connected)
            .then_with(|| a.alias.cmp(&b.alias))
            .then_with(|| a.address.cmp(&b.address))
    });

    sync_rows(rows, infos.into_iter().map(device_row).collect());
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
