use gtk4::{glib, subclass::prelude::*};
use zbus::proxy;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum NetworkState {
    #[default]
    Unknown = 0,
    Asleep = 10,
    Disconnected = 20,
    Disconnecting = 30,
    Connecting = 40,
    ConnectedLocal = 50,
    ConnectedSite = 60,
    ConnectedGlobal = 70,
}

impl From<u32> for NetworkState {
    fn from(value: u32) -> Self {
        match value {
            10 => Self::Asleep,
            20 => Self::Disconnected,
            30 => Self::Disconnecting,
            40 => Self::Connecting,
            50 => Self::ConnectedLocal,
            60 => Self::ConnectedSite,
            70 => Self::ConnectedGlobal,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum DeviceType {
    #[default]
    Unknown = 0,
    Ethernet = 1,
    Wifi = 2,
    Bluetooth = 5,
    Generic = 14,
}

impl From<u32> for DeviceType {
    fn from(value: u32) -> Self {
        match value {
            1 => Self::Ethernet,
            2 => Self::Wifi,
            5 => Self::Bluetooth,
            14 => Self::Generic,
            _ => Self::Unknown,
        }
    }
}

#[proxy(
    interface = "org.freedesktop.NetworkManager",
    default_service = "org.freedesktop.NetworkManager",
    default_path = "/org/freedesktop/NetworkManager"
)]
trait NetworkManager {
    #[zbus(property)]
    fn state(&self) -> zbus::Result<u32>;

    #[zbus(property)]
    fn connectivity(&self) -> zbus::Result<u32>;

    #[zbus(property)]
    fn primary_connection(&self) -> zbus::Result<zbus::zvariant::OwnedObjectPath>;

    fn get_devices(&self) -> zbus::Result<Vec<zbus::zvariant::OwnedObjectPath>>;
}

#[proxy(
    interface = "org.freedesktop.NetworkManager.Device",
    default_service = "org.freedesktop.NetworkManager"
)]
trait NetworkDevice {
    #[zbus(property)]
    fn device_type(&self) -> zbus::Result<u32>;

    #[zbus(property)]
    fn interface(&self) -> zbus::Result<String>;

    #[zbus(property)]
    fn state(&self) -> zbus::Result<u32>;
}

#[proxy(
    interface = "org.freedesktop.NetworkManager.Device.Wireless",
    default_service = "org.freedesktop.NetworkManager"
)]
trait WirelessDevice {
    #[zbus(property)]
    fn active_access_point(&self) -> zbus::Result<zbus::zvariant::OwnedObjectPath>;
}

#[proxy(
    interface = "org.freedesktop.NetworkManager.AccessPoint",
    default_service = "org.freedesktop.NetworkManager"
)]
trait AccessPoint {
    #[zbus(property)]
    fn ssid(&self) -> zbus::Result<Vec<u8>>;

    #[zbus(property)]
    fn strength(&self) -> zbus::Result<u8>;
}

mod imp {
    use std::cell::{Cell, RefCell};

    use anyhow::Result;
    use gtk4::{glib, prelude::*, subclass::prelude::*};
    use zbus::Connection;

    use super::{
        AccessPointProxy, DeviceType, NetworkDeviceProxy, NetworkManagerProxy, NetworkState,
        WirelessDeviceProxy,
    };

    #[derive(glib::Properties, Default)]
    #[properties(wrapper_type = super::NetworkService)]
    pub struct NetworkService {
        #[property(get, set)]
        available: Cell<bool>,

        #[property(get, set)]
        connected: Cell<bool>,

        #[property(get, set)]
        wifi_enabled: Cell<bool>,

        #[property(get, set)]
        wifi_ssid: RefCell<String>,

        #[property(get, set, minimum = 0, maximum = 100)]
        wifi_strength: Cell<u32>,

        #[property(get, set)]
        ethernet_connected: Cell<bool>,

        connection: RefCell<Option<Connection>>,
        state: RefCell<NetworkState>,
        primary_device_type: RefCell<DeviceType>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for NetworkService {
        type ParentType = glib::Object;
        type Type = super::NetworkService;

        const NAME: &'static str = "MuseShellNetworkService";
    }

    #[glib::derived_properties]
    impl ObjectImpl for NetworkService {
        fn constructed(&self) {
            self.parent_constructed();

            // Initialize NetworkManager connection asynchronously
            let obj = self.obj().clone();
            glib::spawn_future_local(async move {
                if let Ok(conn) = Connection::system().await {
                    obj.imp().connection.replace(Some(conn));
                    obj.imp().available.set(true);

                    // Initial state update
                    if let Err(e) = obj.imp().update_network_state().await {
                        log::warn!("Failed to update initial network state: {}", e);
                    }

                    // Start monitoring
                    obj.imp().start_monitoring().await;
                } else {
                    log::warn!(
                        "Failed to connect to NetworkManager D-Bus, network service unavailable"
                    );
                    obj.imp().available.set(false);
                }
            });
        }
    }

    impl NetworkService {
        async fn update_network_state(&self) -> Result<()> {
            let conn = self.connection.borrow().clone();
            let conn = conn
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("No D-Bus connection"))?;

            let nm_proxy = NetworkManagerProxy::new(conn).await?;

            // Get overall state
            let state = NetworkState::from(nm_proxy.state().await?);
            self.state.replace(state);

            let is_connected = matches!(
                state,
                NetworkState::ConnectedLocal
                    | NetworkState::ConnectedSite
                    | NetworkState::ConnectedGlobal
            );
            self.connected.set(is_connected);

            // Get primary connection info
            if is_connected {
                if let Ok(primary_conn) = nm_proxy.primary_connection().await
                    && let Err(e) = self.update_primary_device_info(conn, &primary_conn).await
                {
                    log::warn!("Failed to update primary device info: {}", e);
                }
            } else {
                // Reset connection info when disconnected
                self.wifi_ssid.replace(String::new());
                self.wifi_strength.set(0);
                self.ethernet_connected.set(false);
                self.wifi_enabled.set(false);
            }

            Ok(())
        }

        async fn update_primary_device_info(
            &self,
            conn: &Connection,
            _primary_conn: &zbus::zvariant::OwnedObjectPath,
        ) -> Result<()> {
            let nm_proxy = NetworkManagerProxy::new(conn).await?;
            let devices = nm_proxy.get_devices().await?;

            // Find the active device (simplified - just check first connected device)
            for device_path in devices {
                let device_proxy = NetworkDeviceProxy::builder(conn)
                    .path(&device_path)?
                    .build()
                    .await?;

                let device_type = DeviceType::from(device_proxy.device_type().await?);
                let device_state = device_proxy.state().await?;

                // Device state 100 = activated/connected
                if device_state == 100 {
                    self.primary_device_type.replace(device_type);

                    match device_type {
                        DeviceType::Wifi => {
                            self.wifi_enabled.set(true);
                            self.ethernet_connected.set(false);

                            // Get WiFi specific info
                            if let Err(e) = self.update_wifi_info(conn, &device_path).await {
                                log::warn!("Failed to update WiFi info: {}", e);
                            }
                        }
                        DeviceType::Ethernet => {
                            self.ethernet_connected.set(true);
                            self.wifi_enabled.set(false);
                            self.wifi_ssid.replace(String::new());
                            self.wifi_strength.set(0);
                        }
                        _ => {
                            // Other device types
                            self.wifi_enabled.set(false);
                            self.ethernet_connected.set(false);
                        }
                    }
                    break;
                }
            }

            Ok(())
        }

        async fn update_wifi_info(
            &self,
            conn: &Connection,
            device_path: &zbus::zvariant::OwnedObjectPath,
        ) -> Result<()> {
            let wifi_proxy = WirelessDeviceProxy::builder(conn)
                .path(device_path)?
                .build()
                .await?;

            if let Ok(ap_path) = wifi_proxy.active_access_point().await {
                let ap_proxy = AccessPointProxy::builder(conn)
                    .path(&ap_path)?
                    .build()
                    .await?;

                // Get SSID
                if let Ok(ssid_bytes) = ap_proxy.ssid().await {
                    let ssid = String::from_utf8_lossy(&ssid_bytes).to_string();
                    self.wifi_ssid.replace(ssid);
                }

                // Get signal strength
                if let Ok(strength) = ap_proxy.strength().await {
                    self.wifi_strength.set(strength as u32);
                }
            }

            Ok(())
        }

        async fn start_monitoring(&self) {
            let obj = self.obj().clone();

            // Monitor network changes every 5 seconds
            glib::timeout_add_local(std::time::Duration::from_secs(5), move || {
                let obj_clone = obj.clone();
                glib::spawn_future_local(async move {
                    if let Err(e) = obj_clone.imp().update_network_state().await {
                        log::warn!("Failed to update network state: {}", e);
                    }
                });
                glib::ControlFlow::Continue
            });
        }

        pub fn get_state(&self) -> NetworkState {
            *self.state.borrow()
        }

        pub fn get_primary_device_type(&self) -> DeviceType {
            *self.primary_device_type.borrow()
        }
    }
}

glib::wrapper! {
    pub struct NetworkService(ObjectSubclass<imp::NetworkService>);
}

impl Default for NetworkService {
    fn default() -> Self {
        Self::new()
    }
}

impl NetworkService {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    pub fn state(&self) -> NetworkState {
        self.imp().get_state()
    }

    pub fn primary_device_type(&self) -> DeviceType {
        self.imp().get_primary_device_type()
    }

    pub fn is_wifi_connected(&self) -> bool {
        self.wifi_enabled() && self.connected()
    }

    pub fn is_ethernet_connected(&self) -> bool {
        self.ethernet_connected() && self.connected()
    }

    pub fn connection_type_string(&self) -> String {
        if self.is_wifi_connected() {
            format!("WiFi: {}", self.wifi_ssid())
        } else if self.is_ethernet_connected() {
            "Ethernet".to_string()
        } else {
            "Disconnected".to_string()
        }
    }
}
