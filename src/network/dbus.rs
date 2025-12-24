use zbus::proxy;

use crate::network::types::{ConnectivityState, DeviceState, DeviceStateReason, DeviceType, State};

#[proxy(
    interface = "org.freedesktop.NetworkManager",
    default_service = "org.freedesktop.NetworkManager",
    default_path = "/org/freedesktop/NetworkManager"
)]
pub trait NetworkManager {
    #[zbus(property)]
    fn state(&self) -> zbus::Result<State>;

    #[zbus(property)]
    fn connectivity(&self) -> zbus::Result<ConnectivityState>;

    #[zbus(property)]
    fn primary_connection(&self) -> zbus::Result<zbus::zvariant::OwnedObjectPath>;

    fn get_devices(&self) -> zbus::Result<Vec<zbus::zvariant::OwnedObjectPath>>;
}

#[proxy(
    interface = "org.freedesktop.NetworkManager.Device",
    default_service = "org.freedesktop.NetworkManager"
)]
pub trait NetworkDevice {
    #[zbus(property)]
    fn device_type(&self) -> zbus::Result<DeviceType>;

    #[zbus(property)]
    fn interface(&self) -> zbus::Result<String>;

    #[zbus(property)]
    fn state(&self) -> zbus::Result<DeviceState>;

    #[zbus(property)]
    fn state_reason(&self) -> zbus::Result<DeviceStateReason>;
}

#[proxy(
    interface = "org.freedesktop.NetworkManager.Device.Wireless",
    default_service = "org.freedesktop.NetworkManager"
)]
pub trait WirelessDevice {
    #[zbus(property)]
    fn active_access_point(&self) -> zbus::Result<zbus::zvariant::OwnedObjectPath>;

    /// Request the device to scan.
    fn request_scan(&self) -> zbus::Result<()>;

    /// The bit rate currently used by the wireless device, in kilobits/second
    /// (Kb/s).
    #[zbus(property)]
    fn bitrate(&self) -> zbus::Result<u32>;
}

#[proxy(
    interface = "org.freedesktop.NetworkManager.Device.Wired",
    default_service = "org.freedesktop.NetworkManager"
)]
pub trait WiredDevice {
    /// Design speed of the device, in megabits/second (Mb/s).
    #[zbus(property)]
    fn speed(&self) -> zbus::Result<u32>;

    /// Indicates whether the physical carrier is found (e.g. whether a cable
    /// is plugged in or not).
    #[zbus(property)]
    fn carrier(&self) -> zbus::Result<bool>;
}

#[proxy(
    interface = "org.freedesktop.NetworkManager.AccessPoint",
    default_service = "org.freedesktop.NetworkManager"
)]
pub trait AccessPoint {
    /// Flags describing the capabilities of the access point.
    #[zbus(property)]
    fn flags(&self) -> zbus::Result<u32>;

    /// Flags describing the access point's capabilities according to WPA
    /// (Wifi Protected Access).
    #[zbus(property)]
    fn wpa_flags(&self) -> zbus::Result<u32>;

    /// Flags describing the access point's capabilities according to the RSN
    /// (Robust Secure Network) protocol.
    #[zbus(property)]
    fn rsn_flags(&self) -> zbus::Result<u32>;

    /// The Service Set Identifier identifying the access point.
    #[zbus(property)]
    fn ssid(&self) -> zbus::Result<Vec<u8>>;

    /// The radio channel frequency in use by the access point, in MHz.
    #[zbus(property)]
    fn frequency(&self) -> zbus::Result<u32>;

    /// The hardware address (BSSID) of the access point.
    #[zbus(property)]
    fn hw_address(&self) -> zbus::Result<String>;

    /// Describes the operating mode of the access point.
    #[zbus(property)]
    fn mode(&self) -> zbus::Result<u32>;

    /// The maximum bitrate this access point is capable of, in
    /// kilobits/second (Kb/s).
    #[zbus(property)]
    fn max_bitrate(&self) -> zbus::Result<u32>;

    /// The bandwidth announced by the access point in MHz.
    #[zbus(property)]
    fn bandwidth(&self) -> zbus::Result<u32>;

    /// The current signal quality of the access point, in percent.
    #[zbus(property)]
    fn strength(&self) -> zbus::Result<u8>;

    /// The timestamp (in CLOCK_BOOTTIME seconds) for the last time the access
    /// point was found in scan results. A value of -1 means the access point
    /// has never been found in scan results.
    #[zbus(property)]
    fn last_seen(&self) -> zbus::Result<i32>;
}

#[proxy(
    interface = "org.freedesktop.NetworkManager.Connection.Active",
    default_service = "org.freedesktop.NetworkManager"
)]
pub trait ActiveConnection {
    /// The ID of the connection, provided as a convenience so that clients
    /// do not have to retrieve all connection details.
    #[zbus(property)]
    fn id(&self) -> zbus::Result<String>;

    /// The UUID of the connection, provided as a convenience so that clients
    /// do not have to retrieve all connection details.
    #[zbus(property)]
    fn uuid(&self) -> zbus::Result<String>;

    /// The type of the connection, provided as a convenience so that clients
    /// do not have to retrieve all connection details.
    #[zbus(property, name = "Type")]
    fn type_(&self) -> zbus::Result<String>;

    /// Array of object paths of devices which are part of this active
    /// connection.
    #[zbus(property)]
    fn devices(&self) -> zbus::Result<Vec<zbus::zvariant::OwnedObjectPath>>;

    /// The state of this active connection.
    #[zbus(property)]
    fn state(&self) -> zbus::Result<u32>;

    /// The path of the connection object.
    #[zbus(property)]
    fn connection(&self) -> zbus::Result<zbus::zvariant::OwnedObjectPath>;
}
