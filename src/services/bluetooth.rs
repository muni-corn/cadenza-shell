use bluer::{AdapterEvent, DeviceEvent, Session};
use futures_lite::StreamExt;
use relm4::Worker;

#[derive(Debug, Clone, Default)]
pub struct BluetoothInfo {
    pub enabled: bool,
    pub connected_devices: u32,
    pub available: bool,
}

#[derive(Debug)]
pub enum BluetoothWorkerMsg {
    Update,
}

#[derive(Debug)]
pub enum BluetoothWorkerOutput {
    StateChanged(BluetoothInfo),
}

pub struct BluetoothService;

impl Worker for BluetoothService {
    type Init = ();
    type Input = BluetoothWorkerMsg;
    type Output = BluetoothWorkerOutput;

    fn init(_init: Self::Init, sender: relm4::ComponentSender<Self>) -> Self {
        // Set up Bluetooth monitoring
        let sender_clone = sender.clone();
        relm4::spawn(async move {
            if let Err(e) = Self::setup_bluetooth_monitoring(sender_clone).await {
                log::error!("Failed to setup bluetooth monitoring: {}", e);
            }
        });

        // Initial fetch
        let sender_init = sender.clone();
        relm4::spawn(async move {
            let info = Self::fetch_bluetooth_info().await.unwrap_or_default();
            sender_init
                .output(BluetoothWorkerOutput::StateChanged(info))
                .unwrap_or_else(|e| log::error!("Failed to send initial bluetooth state: {:?}", e));
        });

        Self
    }

    fn update(&mut self, msg: Self::Input, sender: relm4::ComponentSender<Self>) {
        match msg {
            BluetoothWorkerMsg::Update => {
                let sender_clone = sender.clone();
                relm4::spawn(async move {
                    let info = Self::fetch_bluetooth_info().await.unwrap_or_default();
                    sender_clone
                        .output(BluetoothWorkerOutput::StateChanged(info))
                        .unwrap_or_else(|e| log::error!("Failed to send bluetooth state: {:?}", e));
                });
            }
        }
    }
}

impl BluetoothService {
    async fn setup_bluetooth_monitoring(
        sender: relm4::ComponentSender<Self>,
    ) -> anyhow::Result<()> {
        let session = Session::new().await?;
        let adapter = session.default_adapter().await?;

        // Enable the adapter
        adapter.set_powered(true).await?;

        // Monitor adapter events
        let mut adapter_events = adapter.events().await?;
        let sender_adapter = sender.clone();
        relm4::spawn(async move {
            while let Some(event) = adapter_events.next().await {
                if let AdapterEvent::PropertyChanged(_) = event {
                    sender_adapter.input(BluetoothWorkerMsg::Update);
                }
            }
        });

        // Start device discovery
        let mut discovery_events = adapter.discover_devices().await?;
        let sender_discovery = sender.clone();
        relm4::spawn(async move {
            while let Some(event) = discovery_events.next().await {
                match event {
                    AdapterEvent::DeviceAdded(_) | AdapterEvent::DeviceRemoved(_) => {
                        sender_discovery.input(BluetoothWorkerMsg::Update);
                    }
                    _ => {}
                }
            }
        });

        // Monitor existing devices for connection status changes
        let devices = adapter.device_addresses().await.unwrap_or_default();
        for addr in devices {
            if let Ok(device) = adapter.device(addr)
                && let Ok(mut device_events) = device.events().await
            {
                let sender_device = sender.clone();
                relm4::spawn(async move {
                    while let Some(event) = device_events.next().await {
                        let DeviceEvent::PropertyChanged(property) = event;
                        if matches!(property, bluer::DeviceProperty::Connected(_)) {
                            sender_device.input(BluetoothWorkerMsg::Update);
                        }
                    }
                });
            }
        }

        Ok(())
    }

    async fn fetch_bluetooth_info() -> anyhow::Result<BluetoothInfo> {
        match Session::new().await {
            Ok(session) => {
                match session.default_adapter().await {
                    Ok(adapter) => {
                        let powered = adapter.is_powered().await.unwrap_or(false);
                        let mut connected_count = 0u32;

                        // Count connected devices
                        if let Ok(devices) = adapter.device_addresses().await {
                            for addr in devices {
                                if let Ok(device) = adapter.device(addr)
                                    && device.is_connected().await.unwrap_or(false)
                                {
                                    connected_count += 1;
                                }
                            }
                        }

                        Ok(BluetoothInfo {
                            enabled: powered,
                            connected_devices: connected_count,
                            available: true,
                        })
                    }
                    Err(_) => Ok(BluetoothInfo {
                        enabled: false,
                        connected_devices: 0,
                        available: true,
                    }),
                }
            }
            Err(_) => Ok(BluetoothInfo {
                enabled: false,
                connected_devices: 0,
                available: false,
            }),
        }
    }
}
