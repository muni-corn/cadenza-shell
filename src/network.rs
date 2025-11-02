pub mod dbus;
pub mod types;

use futures_lite::StreamExt;
use relm4::{SharedState, Worker, prelude::*};

use crate::network::{
    dbus::{AccessPointProxy, NetworkDeviceProxy, NetworkManagerProxy, WirelessDeviceProxy},
    types::{Connectivity, DeviceState, DeviceType, NetworkState, State},
};

pub static NETWORK_STATE: SharedState<NetworkState> = SharedState::new();

#[derive(Debug)]
pub enum NetworkWorkerMsg {
    Update,
}

#[derive(Debug)]
pub struct NetworkService;

impl Default for NetworkState {
    fn default() -> Self {
        Self {
            connected: false,
            state: State::Unknown,
            connectivity: Connectivity::Unknown,
            device_type: DeviceType::Unknown,
            wifi_ssid: String::new(),
            wifi_strength: 0,
        }
    }
}

impl Worker for NetworkService {
    type Init = ();
    type Input = NetworkWorkerMsg;
    type Output = ();

    fn init(_init: Self::Init, sender: ComponentSender<Self>) -> Self {
        // setup property watching
        let sender_clone = sender.clone();
        relm4::spawn(async move {
            if let Err(e) = Self::setup_property_watching(sender_clone).await {
                log::error!("failed to setup network property watching: {}", e);
            }
        });

        // initial fetch
        relm4::spawn(async move {
            *NETWORK_STATE.write() = Self::fetch_network_info().await.unwrap_or_default();
        });

        Self
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>) {
        match msg {
            NetworkWorkerMsg::Update => {
                relm4::spawn(async move {
                    *NETWORK_STATE.write() = Self::fetch_network_info().await.unwrap_or_default();
                });
            }
        }
    }
}

impl NetworkService {
    async fn setup_property_watching(sender: ComponentSender<Self>) -> anyhow::Result<()> {
        let conn = zbus::Connection::system().await?;
        let nm_proxy = NetworkManagerProxy::new(&conn).await?;

        // watch for state changes
        let mut state_stream = nm_proxy.receive_state_changed().await;
        let sender_state = sender.clone();
        relm4::spawn(async move {
            while let Some(_change) = state_stream.next().await {
                sender_state.input(NetworkWorkerMsg::Update);
            }
        });

        // watch for connectivity changes
        let mut connectivity_stream = nm_proxy.receive_connectivity_changed().await;
        let sender_connectivity = sender.clone();
        relm4::spawn(async move {
            while let Some(_change) = connectivity_stream.next().await {
                sender_connectivity.input(NetworkWorkerMsg::Update);
            }
        });

        // watch for primary connection changes
        let mut primary_connection_stream = nm_proxy.receive_primary_connection_changed().await;
        let sender_primary = sender.clone();
        relm4::spawn(async move {
            while let Some(_change) = primary_connection_stream.next().await {
                sender_primary.input(NetworkWorkerMsg::Update);
            }
        });

        Ok(())
    }

    async fn fetch_network_info() -> anyhow::Result<NetworkState> {
        let conn = zbus::Connection::system().await?;
        let nm_proxy = NetworkManagerProxy::new(&conn).await?;

        // get overall state
        let state = State::from(nm_proxy.state().await?);
        let is_connected = matches!(
            state,
            State::ConnectedLocal | State::ConnectedSite | State::ConnectedGlobal
        );

        // get connectivity
        let connectivity = Connectivity::from(nm_proxy.connectivity().await?);

        let mut info = NetworkState {
            connected: is_connected,
            state,
            connectivity,
            device_type: DeviceType::Unknown,
            wifi_ssid: String::new(),
            wifi_strength: 0,
        };

        if is_connected {
            match nm_proxy.get_devices().await {
                Ok(devices) => {
                    // find the active device
                    for device_path in devices {
                        let device_proxy = NetworkDeviceProxy::builder(&conn)
                            .path(&device_path)?
                            .build()
                            .await?;

                        let device_type = DeviceType::from(device_proxy.device_type().await?);
                        let device_state = DeviceState::from(device_proxy.state().await?);

                        // only activated devices are considered active for primary connection
                        if device_state == DeviceState::Activated {
                            info.device_type = device_type;

                            if device_type == DeviceType::Wifi {
                                // get wifi specific info
                                match Self::get_wifi_info(&conn, &device_path).await {
                                    Ok(wifi_info) => {
                                        info.wifi_ssid = wifi_info.0;
                                        info.wifi_strength = wifi_info.1;
                                        // WiFi info successfully retrieved
                                    }
                                    Err(e) => {
                                        log::error!("failed to get WiFi info: {}", e);
                                        // try to get at least the interface name as fallback
                                        if let Ok(interface) = device_proxy.interface().await {
                                            info.wifi_ssid = interface;
                                        }
                                    }
                                }
                                break; // Found our primary WiFi connection
                            }
                            // For non-WiFi activated devices, set as primary
                            // but continue looking for WiFi
                        } else if device_type == DeviceType::Wifi {
                            // Even if WiFi is not activated, try to get the SSID if connecting
                            if matches!(
                                device_state,
                                DeviceState::Config | DeviceState::IpConfig | DeviceState::IpCheck
                            ) {
                                match Self::get_wifi_info(&conn, &device_path).await {
                                    Ok(wifi_info) => {
                                        info.device_type = device_type;
                                        info.wifi_ssid = wifi_info.0;
                                        info.wifi_strength = wifi_info.1;
                                        log::debug!(
                                            "wifi connecting/configuring: ssid='{}', strength={}",
                                            info.wifi_ssid,
                                            info.wifi_strength
                                        );
                                        break;
                                    }
                                    Err(e) => {
                                        log::debug!(
                                            "wifi device in progress but no ssid available: {}",
                                            e
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    log::error!("failed to get devices: {}", e);
                }
            }
        }

        Ok(info)
    }

    async fn get_wifi_info(
        conn: &zbus::Connection,
        device_path: &zbus::zvariant::OwnedObjectPath,
    ) -> anyhow::Result<(String, u8)> {
        let wifi_proxy = WirelessDeviceProxy::builder(conn)
            .path(device_path)?
            .build()
            .await?;

        let ap_path = wifi_proxy.active_access_point().await?;

        // check if access point path is valid (not "/" which means no connection)
        if ap_path.as_str() == "/" {
            anyhow::bail!("no active access point");
        }

        let ap_proxy = AccessPointProxy::builder(conn)
            .path(&ap_path)?
            .build()
            .await?;

        let ssid_bytes = ap_proxy.ssid().await?;
        let strength = ap_proxy.strength().await?;

        // filter out empty SSID
        if ssid_bytes.is_empty() {
            anyhow::bail!("empty SSID");
        }

        let ssid = String::from_utf8_lossy(&ssid_bytes).to_string();

        // filter out SSIDs that are just whitespace
        if ssid.trim().is_empty() {
            anyhow::bail!("ssid is whitespace only");
        }

        Ok((ssid, strength))
    }
}
