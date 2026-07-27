use std::collections::HashSet;

use bluer::Address;
use gtk4::prelude::*;
use relm4::prelude::*;

use crate::{
    bluetooth::{
        self, BLUETOOTH_STATE, BluetoothState, DeviceInfo, PAIRING_PROMPT, PairingPrompt,
        PairingReply, PairingRequest,
    },
    icon_names::{BLUETOOTH, BLUETOOTH_DOTS, BLUETOOTH_NO, BLUETOOTH_X},
};

#[derive(Debug)]
pub struct BluetoothMenu {
    bluetooth_state: Option<BluetoothState>,
    paired_devices: AsyncFactoryVecDeque<BluetoothDeviceWidget>,
    nearby_devices: AsyncFactoryVecDeque<BluetoothDeviceWidget>,
    pairing_prompt: Option<PairingPrompt>,
}

#[derive(Debug)]
pub enum BluetoothMenuMsg {
    ToggleBluetooth(bool),
    ToggleDiscovery,
    ConnectToDevice(Address),
    DisconnectFromDevice(Address),
    PairDevice(Address),
    RemoveDevice(Address),
    UpdateState(Option<BluetoothState>),
    UpdatePairingPrompt(Option<PairingPrompt>),
    PairingSubmit(String),
    PairingConfirm,
    PairingCancel,
}

#[derive(Debug)]
pub struct BluetoothMenuWidgets {
    icon: gtk::Image,
    toggle_switch: gtk::Switch,
    status_label: gtk::Label,
    scan_button: gtk::Button,
    pairing_box: gtk::Box,
    pairing_label: gtk::Label,
    pairing_entry: gtk::Entry,
    pairing_confirm_button: gtk::Button,
    pairing_cancel_button: gtk::Button,
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
        // create device factories, one per section
        let paired_devices = AsyncFactoryVecDeque::builder()
            .launch(
                gtk::Box::builder()
                    .orientation(gtk::Orientation::Vertical)
                    .spacing(8)
                    .build(),
            )
            .forward(sender.input_sender(), device_output_to_msg);
        let nearby_devices = AsyncFactoryVecDeque::builder()
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
        };

        // create header box
        let header_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(20)
            .hexpand(true)
            .visible(true)
            .css_classes(["content-title"])
            .build();

        // create bluetooth icon
        let icon = gtk::Image::builder()
            .icon_size(gtk::IconSize::Large)
            .hexpand(true)
            .halign(gtk::Align::Start)
            .icon_name(get_icon(&model.bluetooth_state))
            .build();

        // create toggle switch
        let toggle_switch = gtk::Switch::builder()
            .halign(gtk::Align::End)
            .valign(gtk::Align::End)
            .active(model.bluetooth_state.as_ref().map_or_default(|s| s.powered))
            .build();

        // connect toggle switch handler
        toggle_switch.connect_state_set({
            let sender = sender.clone();
            move |_, state| {
                sender.input(BluetoothMenuMsg::ToggleBluetooth(state));
                glib::Propagation::Stop
            }
        });

        header_box.append(&icon);
        header_box.append(&toggle_switch);
        root.append(&header_box);

        // create scrolled window for content
        let scrolled_window = gtk::ScrolledWindow::builder()
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .build();

        // create content box
        let content_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(16)
            .build();

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

        // pairing prompt (initially hidden)
        let pairing_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(8)
            .visible(false)
            .build();

        let pairing_label = gtk::Label::builder()
            .halign(gtk::Align::Start)
            .wrap(true)
            .build();

        let pairing_entry = gtk::Entry::builder()
            .placeholder_text("PIN or passkey")
            .activates_default(true)
            .build();

        let pairing_buttons_box = gtk::Box::builder().spacing(8).build();

        let pairing_cancel_button = gtk::Button::builder().label("Cancel").build();
        pairing_cancel_button.connect_clicked({
            let sender = sender.clone();
            move |_| sender.input(BluetoothMenuMsg::PairingCancel)
        });

        let pairing_confirm_button = gtk::Button::builder().label("Confirm").build();
        // the entry is only visible for PinCode/Passkey requests; for
        // ConfirmPasskey/Authorize it's hidden and there's nothing to submit,
        // just a yes/no
        let submit = {
            let sender = sender.clone();
            let pairing_entry = pairing_entry.clone();
            move || {
                if pairing_entry.get_visible() {
                    sender.input(BluetoothMenuMsg::PairingSubmit(
                        pairing_entry.text().to_string(),
                    ));
                } else {
                    sender.input(BluetoothMenuMsg::PairingConfirm);
                }
            }
        };
        pairing_confirm_button.connect_clicked({
            let submit = submit.clone();
            move |_| submit()
        });
        pairing_entry.connect_activate(move |_| submit());

        pairing_buttons_box.append(&pairing_cancel_button);
        pairing_buttons_box.append(&pairing_confirm_button);

        pairing_box.append(&pairing_label);
        pairing_box.append(&pairing_entry);
        pairing_box.append(&pairing_buttons_box);

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
        content_box.append(&pairing_box);
        content_box.append(&paired_header);
        content_box.append(model.paired_devices.widget());
        content_box.append(&nearby_header);
        content_box.append(model.nearby_devices.widget());
        scrolled_window.set_child(Some(&content_box));
        root.append(&scrolled_window);

        // subscribe to bluetooth state and pairing prompt updates
        BLUETOOTH_STATE.subscribe(sender.input_sender(), |state| {
            BluetoothMenuMsg::UpdateState(state.to_owned())
        });
        PAIRING_PROMPT.subscribe(sender.input_sender(), |prompt| {
            BluetoothMenuMsg::UpdatePairingPrompt(prompt.to_owned())
        });

        let mut widgets = BluetoothMenuWidgets {
            icon,
            toggle_switch,
            status_label,
            scan_button,
            pairing_box,
            pairing_label,
            pairing_entry,
            pairing_confirm_button,
            pairing_cancel_button,
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

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            BluetoothMenuMsg::UpdateState(state) => {
                update_device_rows(&mut self.paired_devices, paired_devices_iter(&state));
                update_device_rows(&mut self.nearby_devices, nearby_devices_iter(&state));
                self.bluetooth_state = state;
            }
            BluetoothMenuMsg::UpdatePairingPrompt(prompt) => {
                self.pairing_prompt = prompt;
            }
            BluetoothMenuMsg::ToggleBluetooth(enabled) => {
                let state_clone = self.bluetooth_state.clone();
                sender.oneshot_command(async move {
                    if let Some(state) = state_clone
                        && let Err(e) = state.adapter.set_powered(enabled).await
                    {
                        tracing::error!("failed to toggle bluetooth: {}", e);
                    }
                });
            }
            BluetoothMenuMsg::ToggleDiscovery => {
                let discovering = self.bluetooth_state.as_ref().is_some_and(|s| s.discovering);
                if discovering {
                    bluetooth::stop_discovery();
                } else {
                    bluetooth::start_discovery();
                }
            }
            BluetoothMenuMsg::ConnectToDevice(addr) => {
                let state_clone = self.bluetooth_state.clone();
                sender.oneshot_command(async move {
                    let Some(state) = state_clone else { return };
                    match state.device_handle(addr) {
                        Ok(device) => {
                            if let Err(e) = device.connect().await {
                                tracing::error!("failed to connect to device {addr}: {e}");
                            }
                        }
                        Err(e) => tracing::error!("couldn't build device handle for {addr}: {e}"),
                    }
                });
            }
            BluetoothMenuMsg::DisconnectFromDevice(addr) => {
                let state_clone = self.bluetooth_state.clone();
                sender.oneshot_command(async move {
                    let Some(state) = state_clone else { return };
                    match state.device_handle(addr) {
                        Ok(device) => {
                            if let Err(e) = device.disconnect().await {
                                tracing::error!("failed to disconnect from device {addr}: {e}");
                            }
                        }
                        Err(e) => tracing::error!("couldn't build device handle for {addr}: {e}"),
                    }
                });
            }
            BluetoothMenuMsg::PairDevice(addr) => {
                bluetooth::pair(addr);
            }
            BluetoothMenuMsg::RemoveDevice(addr) => {
                bluetooth::remove(addr);
            }
            BluetoothMenuMsg::PairingSubmit(text) => {
                if let Some(prompt) = &self.pairing_prompt {
                    match prompt.request {
                        PairingRequest::PinCode => {
                            bluetooth::pairing_reply(PairingReply::Text(text));
                        }
                        PairingRequest::Passkey => match text.parse::<u32>() {
                            Ok(passkey) => {
                                bluetooth::pairing_reply(PairingReply::Number(passkey));
                            }
                            Err(_) => {
                                tracing::debug!("passkey entry '{text}' isn't a valid number");
                            }
                        },
                        // the entry isn't shown for these; nothing to submit
                        PairingRequest::ConfirmPasskey(_)
                        | PairingRequest::Authorize
                        | PairingRequest::DisplayPinCode(_)
                        | PairingRequest::DisplayPasskey { .. } => {}
                    }
                }
            }
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
            .icon
            .set_icon_name(Some(get_icon(&self.bluetooth_state)));

        let is_powered = self.bluetooth_state.as_ref().map_or_default(|s| s.powered);
        widgets.toggle_switch.set_active(is_powered);

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
        gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(16)
            .vexpand(true)
            .width_request(320)
            .build()
    }
}

fn device_output_to_msg(output: BluetoothDeviceOutput) -> BluetoothMenuMsg {
    match output {
        BluetoothDeviceOutput::Connect(addr) => BluetoothMenuMsg::ConnectToDevice(addr),
        BluetoothDeviceOutput::Disconnect(addr) => BluetoothMenuMsg::DisconnectFromDevice(addr),
        BluetoothDeviceOutput::Pair(addr) => BluetoothMenuMsg::PairDevice(addr),
        BluetoothDeviceOutput::Remove(addr) => BluetoothMenuMsg::RemoveDevice(addr),
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
        widgets.pairing_box.set_visible(false);
        widgets.pairing_entry.set_text("");
        return;
    };

    widgets.pairing_box.set_visible(true);

    let (label, show_entry, show_buttons) = match &prompt.request {
        PairingRequest::PinCode => (
            format!("Enter the PIN code for {}", prompt.address),
            true,
            true,
        ),
        PairingRequest::Passkey => (
            format!("Enter the passkey for {}", prompt.address),
            true,
            true,
        ),
        PairingRequest::ConfirmPasskey(passkey) => (
            format!("Does {} show the passkey {passkey:06}?", prompt.address),
            false,
            true,
        ),
        PairingRequest::Authorize => (
            format!("Allow pairing with {}?", prompt.address),
            false,
            true,
        ),
        PairingRequest::DisplayPinCode(pin) => (
            format!("PIN code for {}: {pin}", prompt.address),
            false,
            false,
        ),
        PairingRequest::DisplayPasskey { passkey, entered } => (
            format!(
                "Passkey for {}: {passkey:06} ({entered} digit(s) entered)",
                prompt.address
            ),
            false,
            false,
        ),
    };

    widgets.pairing_label.set_label(&label);
    widgets.pairing_entry.set_visible(show_entry);
    widgets.pairing_confirm_button.set_visible(show_buttons);
    widgets.pairing_cancel_button.set_visible(show_buttons);
}

fn get_icon(state: &Option<BluetoothState>) -> &str {
    match state {
        Some(s) if s.powered && s.discovering => BLUETOOTH_DOTS,
        Some(s) if s.powered && s.connected_device_count() > 0 => BLUETOOTH,
        Some(s) if s.powered => BLUETOOTH_X,
        _ => BLUETOOTH_NO,
    }
}

fn get_status_text(state: &Option<BluetoothState>) -> String {
    match state {
        Some(s) if !s.powered => "Bluetooth disabled".to_string(),
        Some(s) if s.discovering => "Searching for devices...".to_string(),
        Some(s) if s.connected_device_count() == 1 => "1 device connected".to_string(),
        Some(s) if s.connected_device_count() > 0 => {
            format!("{} device(s) connected", s.connected_device_count())
        }
        Some(_) => "Bluetooth enabled".to_string(),
        None => "Bluetooth unavailable".to_string(),
    }
}

/// Diffs the factory's current rows against the latest device snapshots:
/// removes rows for devices that disappeared, updates existing rows in
/// place, and inserts new ones - instead of clearing and rebuilding the
/// whole list on every state update. Since that update fires as often as
/// every reconcile tick, a clear-and-rebuild would destroy and recreate
/// every row's widgets on a timer even when nothing about them changed.
fn update_device_rows<'a>(
    devices: &mut AsyncFactoryVecDeque<BluetoothDeviceWidget>,
    infos: impl Iterator<Item = &'a DeviceInfo>,
) {
    let infos: Vec<&DeviceInfo> = infos.collect();
    let new_addresses: HashSet<Address> = infos.iter().map(|d| d.address).collect();

    // remove rows whose device disappeared (or moved to the other section);
    // walk in reverse so removing a later index doesn't invalidate earlier
    // ones still to be checked
    let current_addresses: Vec<Option<Address>> =
        devices.iter().map(|w| w.map(|w| w.info.address)).collect();
    {
        let mut guard = devices.guard();
        for (index, address) in current_addresses.iter().enumerate().rev() {
            let is_stale = address.is_some_and(|addr| !new_addresses.contains(&addr));
            if is_stale {
                guard.remove(index);
            }
        }
    }

    // update rows that still exist, insert ones that are new
    for info in infos {
        let existing_index = devices
            .iter()
            .position(|w| w.map(|w| w.info.address) == Some(info.address));

        match existing_index {
            Some(index) => devices.send(index, BluetoothDeviceMsg::UpdateInfo(info.clone())),
            None => {
                devices.guard().push_back(info.clone());
            }
        }
    }
}

// factory for individual device items
#[derive(Debug)]
struct BluetoothDeviceWidget {
    info: DeviceInfo,
}

#[derive(Debug)]
pub enum BluetoothDeviceMsg {
    Toggle,
    RemoveClicked,
    UpdateInfo(DeviceInfo),
}

#[derive(Debug)]
pub enum BluetoothDeviceOutput {
    Connect(Address),
    Disconnect(Address),
    Pair(Address),
    Remove(Address),
}

pub struct BluetoothDeviceWidgetWidgets {
    _main_box: gtk::Box,
    device_label: gtk::Label,
    status_label: gtk::Label,
    remove_button: gtk::Button,
}

impl AsyncFactoryComponent for BluetoothDeviceWidget {
    type CommandOutput = ();
    type Init = DeviceInfo;
    type Input = BluetoothDeviceMsg;
    type Output = BluetoothDeviceOutput;
    type ParentWidget = gtk::Box;
    type Root = gtk::Box;
    type Widgets = BluetoothDeviceWidgetWidgets;

    async fn init_model(
        init: Self::Init,
        _index: &DynamicIndex,
        _sender: AsyncFactorySender<Self>,
    ) -> Self {
        // no async fetch needed; the snapshot already carries everything we
        // render
        Self { info: init }
    }

    async fn update(&mut self, msg: Self::Input, sender: AsyncFactorySender<Self>) {
        match msg {
            BluetoothDeviceMsg::Toggle => {
                let addr = self.info.address;
                if !self.info.paired {
                    let _ = sender.output(BluetoothDeviceOutput::Pair(addr));
                } else if self.info.connected {
                    let _ = sender.output(BluetoothDeviceOutput::Disconnect(addr));
                } else {
                    let _ = sender.output(BluetoothDeviceOutput::Connect(addr));
                }
            }
            BluetoothDeviceMsg::RemoveClicked => {
                let _ = sender.output(BluetoothDeviceOutput::Remove(self.info.address));
            }
            BluetoothDeviceMsg::UpdateInfo(info) => {
                self.info = info;
            }
        }
    }

    fn init_root() -> Self::Root {
        gtk::Box::builder().spacing(4).build()
    }

    fn init_widgets(
        &mut self,
        _index: &DynamicIndex,
        root: Self::Root,
        _returned_widget: &<Self::ParentWidget as relm4::factory::FactoryView>::ReturnedWidget,
        sender: AsyncFactorySender<Self>,
    ) -> Self::Widgets {
        let select_button = gtk::Button::builder().hexpand(true).build();
        select_button.connect_clicked({
            let sender = sender.clone();
            move |_| sender.input(BluetoothDeviceMsg::Toggle)
        });

        let main_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .halign(gtk::Align::Start)
            .hexpand(true)
            .build();

        let device_label = gtk::Label::builder()
            .halign(gtk::Align::Start)
            .hexpand(true)
            .label(&self.info.alias)
            .build();

        let status_label = gtk::Label::builder()
            .css_classes(["dim"])
            .halign(gtk::Align::End)
            .label("Connected")
            .visible(self.info.connected)
            .build();

        main_box.append(&device_label);
        main_box.append(&status_label);

        select_button.set_child(Some(&main_box));

        let remove_button = gtk::Button::builder()
            .icon_name("user-trash-symbolic")
            .tooltip_text("Remove this device")
            .visible(self.info.paired)
            .build();
        remove_button.connect_clicked(move |_| sender.input(BluetoothDeviceMsg::RemoveClicked));

        root.append(&select_button);
        root.append(&remove_button);

        BluetoothDeviceWidgetWidgets {
            _main_box: main_box,
            device_label,
            status_label,
            remove_button,
        }
    }

    fn update_view(&self, widgets: &mut Self::Widgets, _sender: AsyncFactorySender<Self>) {
        widgets.device_label.set_label(&self.info.alias);
        widgets.status_label.set_visible(self.info.connected);
        widgets.remove_button.set_visible(self.info.paired);
    }
}
