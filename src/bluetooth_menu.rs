use bluer::{Address, Device};
use gtk4::prelude::*;
use relm4::prelude::*;

use crate::{
    bluetooth::{BLUETOOTH_STATE, BluetoothState},
    icon_names::{
        BLUETOOTH_CONNECTED_REGULAR, BLUETOOTH_DISABLED_REGULAR, BLUETOOTH_REGULAR,
        BLUETOOTH_SEARCHING_REGULAR,
    },
};

#[derive(Debug)]
pub struct BluetoothMenu {
    bluetooth_state: Option<BluetoothState>,
    devices: AsyncFactoryVecDeque<BluetoothDeviceWidget>,
}

#[derive(Debug)]
pub enum BluetoothMenuMsg {
    ToggleBluetooth(bool),
    ConnectToDevice(Address),
    DisconnectFromDevice(Address),
    UpdateState(Option<BluetoothState>),
}

#[derive(Debug)]
pub struct BluetoothMenuWidgets {
    icon: gtk::Image,
    toggle_switch: gtk::Switch,
    status_label: gtk::Label,
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
        // create devices factory
        let devices = AsyncFactoryVecDeque::builder()
            .launch(
                gtk::Box::builder()
                    .orientation(gtk::Orientation::Vertical)
                    .spacing(8)
                    .build(),
            )
            .forward(sender.input_sender(), |output| match output {
                BluetoothDeviceOutput::Connect(addr) => BluetoothMenuMsg::ConnectToDevice(addr),
                BluetoothDeviceOutput::Disconnect(addr) => {
                    BluetoothMenuMsg::DisconnectFromDevice(addr)
                }
            });

        let model = BluetoothMenu {
            bluetooth_state: BLUETOOTH_STATE.read().clone(),
            devices,
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

        // create status info box
        let status_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(4)
            .build();

        // create status label
        let status_label = gtk::Label::builder()
            .halign(gtk::Align::Start)
            .label(get_status_text(&model.bluetooth_state))
            .build();

        status_box.append(&status_label);
        content_box.append(&status_box);
        content_box.append(model.devices.widget());
        scrolled_window.set_child(Some(&content_box));
        root.append(&scrolled_window);

        // subscribe to bluetooth state updates
        BLUETOOTH_STATE.subscribe(sender.input_sender(), |state| {
            BluetoothMenuMsg::UpdateState(state.to_owned())
        });

        let widgets = BluetoothMenuWidgets {
            icon,
            toggle_switch,
            status_label,
        };

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            BluetoothMenuMsg::UpdateState(state) => {
                self.bluetooth_state = state.clone();

                // update device list
                let mut devices_guard = self.devices.guard();
                devices_guard.clear();

                if let Some(ref state) = state {
                    for device in state.devices() {
                        devices_guard.push_back(device.clone());
                    }
                }
            }
            BluetoothMenuMsg::ToggleBluetooth(enabled) => {
                let state_clone = self.bluetooth_state.clone();
                sender.oneshot_command(async move {
                    if let Some(state) = state_clone
                        && let Err(e) = state.adapter.set_powered(enabled).await
                    {
                        log::error!("failed to toggle bluetooth: {}", e);
                    }
                });
            }
            BluetoothMenuMsg::ConnectToDevice(addr) => {
                let state_clone = self.bluetooth_state.clone();
                sender.oneshot_command(async move {
                    if let Some(state) = state_clone
                        && let Some(device) = state.get_device(&addr)
                        && let Err(e) = device.connect().await
                    {
                        log::error!("failed to connect to device {}: {}", addr, e);
                    }
                });
            }
            BluetoothMenuMsg::DisconnectFromDevice(addr) => {
                let state_clone = self.bluetooth_state.clone();
                sender.oneshot_command(async move {
                    if let Some(state) = state_clone
                        && let Some(device) = state.get_device(&addr)
                        && let Err(e) = device.disconnect().await
                    {
                        log::error!("failed to disconnect from device {}: {}", addr, e);
                    }
                });
            }
        }
    }

    fn update_view(&self, widgets: &mut Self::Widgets, _sender: ComponentSender<Self>) {
        // update icon
        widgets
            .icon
            .set_icon_name(Some(get_icon(&self.bluetooth_state)));

        // update toggle switch
        let is_powered = self.bluetooth_state.as_ref().map_or_default(|s| s.powered);
        widgets.toggle_switch.set_active(is_powered);

        // update status label
        widgets
            .status_label
            .set_label(&get_status_text(&self.bluetooth_state));
    }

    fn init_root() -> Self::Root {
        // set up the main container
        gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(16)
            .vexpand(true)
            // .width_request(256)
            .height_request(512)
            .build()
    }
}

fn get_icon(state: &Option<BluetoothState>) -> &str {
    match state {
        Some(s) if s.powered && s.discovering => BLUETOOTH_SEARCHING_REGULAR,
        Some(s) if s.powered && s.connected_device_count > 0 => BLUETOOTH_CONNECTED_REGULAR,
        Some(s) if s.powered => BLUETOOTH_REGULAR,
        _ => BLUETOOTH_DISABLED_REGULAR,
    }
}

fn get_status_text(state: &Option<BluetoothState>) -> String {
    match state {
        Some(s) if !s.powered => "Bluetooth disabled".to_string(),
        Some(s) if s.discovering => "Searching for devices...".to_string(),
        Some(s) if s.connected_device_count == 1 => "1 device connected".to_string(),
        Some(s) if s.connected_device_count > 0 => {
            format!("{} device(s) connected", s.connected_device_count)
        }
        Some(_) => "Bluetooth enabled".to_string(),
        None => "Bluetooth unavailable".to_string(),
    }
}

// factory for individual device items
#[derive(Debug)]
struct BluetoothDeviceWidget {
    device: Device,
    name: Option<String>,
    is_connected: bool,
}

#[derive(Debug)]
pub enum BluetoothDeviceMsg {
    Toggle,
    UpdateInfo(Option<String>, bool),
}

#[derive(Debug)]
pub enum BluetoothDeviceOutput {
    Connect(Address),
    Disconnect(Address),
}

pub struct BluetoothDeviceWidgetWidgets {
    main_box: gtk::Box,
    device_label: gtk::Label,
    status_label: gtk::Label,
}

impl AsyncFactoryComponent for BluetoothDeviceWidget {
    type CommandOutput = BluetoothDeviceMsg;
    type Init = Device;
    type Input = BluetoothDeviceMsg;
    type Output = BluetoothDeviceOutput;
    type ParentWidget = gtk::Box;
    type Root = gtk::Button;
    type Widgets = BluetoothDeviceWidgetWidgets;

    async fn init_model(
        init: Self::Init,
        _index: &DynamicIndex,
        sender: AsyncFactorySender<Self>,
    ) -> Self {
        let device = init;

        // fetch device info
        let name = device.name().await.ok().flatten();
        let is_connected = device.is_connected().await.unwrap_or(false);

        // send update message to self
        sender.input(BluetoothDeviceMsg::UpdateInfo(name.clone(), is_connected));

        Self {
            device,
            name: None,
            is_connected: false,
        }
    }

    async fn update(&mut self, msg: Self::Input, sender: AsyncFactorySender<Self>) {
        match msg {
            BluetoothDeviceMsg::Toggle => {
                let addr = self.device.address();
                if self.is_connected {
                    let _ = sender.output(BluetoothDeviceOutput::Disconnect(addr));
                } else {
                    let _ = sender.output(BluetoothDeviceOutput::Connect(addr));
                }
            }
            BluetoothDeviceMsg::UpdateInfo(name, is_connected) => {
                self.name = name;
                self.is_connected = is_connected;
            }
        }
    }

    fn init_root() -> Self::Root {
        gtk::Button::builder().build()
    }

    fn init_widgets(
        &mut self,
        _index: &DynamicIndex,
        root: Self::Root,
        _returned_widget: &<Self::ParentWidget as relm4::factory::FactoryView>::ReturnedWidget,
        sender: AsyncFactorySender<Self>,
    ) -> Self::Widgets {
        root.connect_clicked(move |_| sender.input(BluetoothDeviceMsg::Toggle));

        let main_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(8)
            .halign(gtk::Align::Start)
            .hexpand(true)
            .build();

        let device_label = gtk::Label::builder()
            .halign(gtk::Align::Start)
            .hexpand(true)
            .build();

        let status_label = gtk::Label::builder()
            .css_classes(["dim"])
            .halign(gtk::Align::End)
            .label("Connected")
            .visible(self.is_connected)
            .build();

        main_box.append(&device_label);
        main_box.append(&status_label);

        root.set_child(Some(&main_box));

        BluetoothDeviceWidgetWidgets {
            main_box,
            device_label,
            status_label,
        }
    }

    fn update_view(&self, widgets: &mut Self::Widgets, _sender: AsyncFactorySender<Self>) {
        let device_name = self
            .name
            .as_ref()
            .map(String::from)
            .unwrap_or(self.device.address().to_string());

        widgets.device_label.set_label(&device_name);
        widgets.status_label.set_visible(self.is_connected);
    }
}
