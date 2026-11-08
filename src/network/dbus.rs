//! Hand-written NetworkManager D-Bus proxies, limited to the members the
//! shell uses.

use std::collections::HashMap;

use zbus::{proxy, zvariant};

use crate::network::types::{ConnectivityState, State};

#[proxy(
    interface = "org.freedesktop.NetworkManager",
    default_service = "org.freedesktop.NetworkManager",
    default_path = "/org/freedesktop/NetworkManager"
)]
pub trait NetworkManager {
    /// Get the list of realized network devices.
    fn get_devices(&self) -> zbus::Result<Vec<zvariant::OwnedObjectPath>>;

    /// Activate a connection using the supplied device.
    fn activate_connection(
        &self,
        connection: &zvariant::ObjectPath<'_>,
        device: &zvariant::ObjectPath<'_>,
        specific_object: &zvariant::ObjectPath<'_>,
    ) -> zbus::Result<zvariant::OwnedObjectPath>;

    /// Adds a new connection and activates it.
    fn add_and_activate_connection(
        &self,
        connection: HashMap<String, HashMap<String, zvariant::Value<'_>>>,
        device: &zvariant::ObjectPath<'_>,
        specific_object: &zvariant::ObjectPath<'_>,
    ) -> zbus::Result<(zvariant::OwnedObjectPath, zvariant::OwnedObjectPath)>;

    /// Deactivate an active connection.
    fn deactivate_connection(
        &self,
        active_connection: &zvariant::ObjectPath<'_>,
    ) -> zbus::Result<()>;

    /// Indicates if wireless is currently enabled or not.
    #[zbus(property)]
    fn wireless_enabled(&self) -> zbus::Result<bool>;

    /// Set whether wireless is enabled.
    #[zbus(property)]
    fn set_wireless_enabled(&self, value: bool) -> zbus::Result<()>;

    /// The object path of the "primary" active connection being used to access
    /// the network.
    #[zbus(property)]
    fn primary_connection(&self) -> zbus::Result<zvariant::OwnedObjectPath>;

    /// The object path of an active connection that is currently being
    /// activated.
    #[zbus(property)]
    fn activating_connection(&self) -> zbus::Result<zvariant::OwnedObjectPath>;

    /// The overall state of the NetworkManager daemon.
    #[zbus(property)]
    fn state(&self) -> zbus::Result<State>;

    /// The result of the last connectivity check.
    #[zbus(property)]
    fn connectivity(&self) -> zbus::Result<ConnectivityState>;
}

#[proxy(
    interface = "org.freedesktop.NetworkManager.Device",
    default_service = "org.freedesktop.NetworkManager"
)]
pub trait NetworkDevice {
    /// The device's `NMDeviceType`.
    #[zbus(property)]
    fn device_type(&self) -> zbus::Result<u32>;
}

#[proxy(
    interface = "org.freedesktop.NetworkManager.Device.Wireless",
    default_service = "org.freedesktop.NetworkManager"
)]
pub trait WirelessDevice {
    #[zbus(property)]
    fn active_access_point(&self) -> zbus::Result<zvariant::OwnedObjectPath>;

    /// Request the device to scan.
    ///
    /// `options` is currently always empty (no supported keys are needed for
    /// a plain rescan), but the D-Bus method itself takes a mandatory
    /// `a{sv}` argument, so it must be passed even if empty.
    fn request_scan(&self, options: HashMap<String, zvariant::Value<'_>>) -> zbus::Result<()>;

    /// Get the list of all access points known to this device, including
    /// ones with hidden SSIDs.
    fn get_all_access_points(&self) -> zbus::Result<Vec<zvariant::OwnedObjectPath>>;

    /// The timestamp (in CLOCK_BOOTTIME milliseconds) for the last time a
    /// scan was completed. A value of -1 means the device never scanned for
    /// access points.
    #[zbus(property)]
    fn last_scan(&self) -> zbus::Result<i64>;
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

    /// The current signal quality of the access point, in percent.
    #[zbus(property)]
    fn strength(&self) -> zbus::Result<u8>;
}

#[proxy(
    interface = "org.freedesktop.NetworkManager.Connection.Active",
    default_service = "org.freedesktop.NetworkManager"
)]
pub trait ActiveConnection {
    /// Array of object paths of devices which are part of this active
    /// connection.
    #[zbus(property)]
    fn devices(&self) -> zbus::Result<Vec<zvariant::OwnedObjectPath>>;

    /// The `NMActiveConnectionState` of this active connection.
    #[zbus(property)]
    fn state(&self) -> zbus::Result<u32>;

    /// Emitted whenever the active connection's state changes, with the
    /// `NMActiveConnectionStateReason` for the transition.
    ///
    /// Named differently from the `State` property's auto-generated
    /// `receive_state_changed` to avoid colliding with it.
    #[zbus(signal, name = "StateChanged")]
    fn active_state_changed(&self, state: u32, reason: u32) -> zbus::Result<()>;
}

#[proxy(
    interface = "org.freedesktop.NetworkManager.Settings",
    default_service = "org.freedesktop.NetworkManager",
    default_path = "/org/freedesktop/NetworkManager/Settings"
)]
pub trait Settings {
    /// Returns the list of all known saved connection profiles.
    fn list_connections(&self) -> zbus::Result<Vec<zvariant::OwnedObjectPath>>;
}

#[proxy(
    interface = "org.freedesktop.NetworkManager.Settings.Connection",
    default_service = "org.freedesktop.NetworkManager"
)]
pub trait SettingsConnection {
    /// Returns the settings of this connection profile.
    fn get_settings(&self) -> zbus::Result<HashMap<String, HashMap<String, zvariant::OwnedValue>>>;

    /// Deletes this connection profile.
    fn delete(&self) -> zbus::Result<()>;
}
