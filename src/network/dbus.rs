use std::collections::HashMap;

use zbus::{proxy, zvariant};

use crate::network::types::{ConnectivityState, DeviceState, DeviceStateReason, DeviceType, State};

#[proxy(
    interface = "org.freedesktop.NetworkManager",
    default_service = "org.freedesktop.NetworkManager",
    default_path = "/org/freedesktop/NetworkManager"
)]
pub trait NetworkManager {
    /// Reload NetworkManager's configuration and perform certain updates.
    fn reload(&self, flags: u32) -> zbus::Result<()>;

    /// Get the list of realized network devices.
    fn get_devices(&self) -> zbus::Result<Vec<zvariant::OwnedObjectPath>>;

    /// Get the list of all network devices.
    fn get_all_devices(&self) -> zbus::Result<Vec<zvariant::OwnedObjectPath>>;

    /// Return the object path of the network device referenced by its IP
    /// interface name.
    fn get_device_by_ip_iface(&self, iface: &str) -> zbus::Result<zvariant::OwnedObjectPath>;

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
        connection: HashMap<String, std::collections::HashMap<String, zvariant::Value<'_>>>,
        device: &zvariant::ObjectPath<'_>,
        specific_object: &zvariant::ObjectPath<'_>,
    ) -> zbus::Result<(zvariant::OwnedObjectPath, zvariant::OwnedObjectPath)>;

    /// Adds a new connection and activates it with additional options.
    fn add_and_activate_connection2(
        &self,
        connection: HashMap<String, std::collections::HashMap<String, zvariant::Value<'_>>>,
        device: &zvariant::ObjectPath<'_>,
        specific_object: &zvariant::ObjectPath<'_>,
        options: HashMap<String, zvariant::Value<'_>>,
    ) -> zbus::Result<(
        zvariant::OwnedObjectPath,
        zvariant::OwnedObjectPath,
        HashMap<String, zvariant::OwnedValue>,
    )>;

    /// Deactivate an active connection.
    fn deactivate_connection(
        &self,
        active_connection: &zvariant::ObjectPath<'_>,
    ) -> zbus::Result<()>;

    /// Control the NetworkManager daemon's sleep state.
    fn sleep(&self, sleep: bool) -> zbus::Result<()>;

    /// Control whether overall networking is enabled or disabled.
    fn enable(&self, enable: bool) -> zbus::Result<()>;

    /// Returns the permissions a caller has for various authenticated
    /// operations.
    fn get_permissions(&self) -> zbus::Result<HashMap<String, String>>;

    /// Set logging verbosity and which operations are logged.
    fn set_logging(&self, level: &str, domains: &str) -> zbus::Result<()>;

    /// Get current logging verbosity level and operations domains.
    fn get_logging(&self) -> zbus::Result<(String, String)>;

    /// Re-check the network connectivity state.
    fn check_connectivity(&self) -> zbus::Result<u32>;

    /// Create a checkpoint of the current networking configuration for given
    /// interfaces.
    fn checkpoint_create(
        &self,
        devices: &[zvariant::ObjectPath<'_>],
        rollback_timeout: u32,
        flags: u32,
    ) -> zbus::Result<zvariant::OwnedObjectPath>;

    /// Destroy a previously created checkpoint.
    fn checkpoint_destroy(&self, checkpoint: &zvariant::ObjectPath<'_>) -> zbus::Result<()>;

    /// Rollback a checkpoint before the timeout is reached.
    fn checkpoint_rollback(
        &self,
        checkpoint: &zvariant::ObjectPath<'_>,
    ) -> zbus::Result<HashMap<String, u32>>;

    /// Reset the timeout for rollback for the checkpoint.
    fn checkpoint_adjust_rollback_timeout(
        &self,
        checkpoint: &zvariant::ObjectPath<'_>,
        add_timeout: u32,
    ) -> zbus::Result<()>;

    // properties

    /// The list of realized network devices.
    #[zbus(property)]
    fn devices(&self) -> zbus::Result<Vec<zvariant::OwnedObjectPath>>;

    /// The list of both realized and un-realized network devices.
    #[zbus(property)]
    fn all_devices(&self) -> zbus::Result<Vec<zvariant::OwnedObjectPath>>;

    /// The list of active checkpoints.
    #[zbus(property)]
    fn checkpoints(&self) -> zbus::Result<Vec<zvariant::OwnedObjectPath>>;

    /// Indicates if overall networking is currently enabled or not.
    #[zbus(property)]
    fn networking_enabled(&self) -> zbus::Result<bool>;

    /// Indicates if wireless is currently enabled or not.
    #[zbus(property)]
    fn wireless_enabled(&self) -> zbus::Result<bool>;

    /// Set whether wireless is enabled.
    #[zbus(property)]
    fn set_wireless_enabled(&self, value: bool) -> zbus::Result<()>;

    /// Indicates if the wireless hardware is currently enabled.
    #[zbus(property)]
    fn wireless_hardware_enabled(&self) -> zbus::Result<bool>;

    /// Indicates if mobile broadband devices are currently enabled or not.
    #[zbus(property)]
    fn wwan_enabled(&self) -> zbus::Result<bool>;

    /// Set whether mobile broadband devices are enabled.
    #[zbus(property)]
    fn set_wwan_enabled(&self, value: bool) -> zbus::Result<()>;

    /// Indicates if the mobile broadband hardware is currently enabled.
    #[zbus(property)]
    fn wwan_hardware_enabled(&self) -> zbus::Result<bool>;

    /// Flags related to radio devices.
    #[zbus(property)]
    fn radio_flags(&self) -> zbus::Result<u32>;

    /// List of active connection object paths.
    #[zbus(property)]
    fn active_connections(&self) -> zbus::Result<Vec<zvariant::OwnedObjectPath>>;

    /// The object path of the "primary" active connection being used to access
    /// the network.
    #[zbus(property)]
    fn primary_connection(&self) -> zbus::Result<zvariant::OwnedObjectPath>;

    /// The connection type of the "primary" active connection.
    #[zbus(property)]
    fn primary_connection_type(&self) -> zbus::Result<String>;

    /// Indicates whether the connectivity is metered.
    #[zbus(property)]
    fn metered(&self) -> zbus::Result<u32>;

    /// The object path of an active connection that is currently being
    /// activated.
    #[zbus(property)]
    fn activating_connection(&self) -> zbus::Result<zvariant::OwnedObjectPath>;

    /// Indicates whether NM is still starting up.
    #[zbus(property)]
    fn startup(&self) -> zbus::Result<bool>;

    /// NetworkManager version.
    #[zbus(property)]
    fn version(&self) -> zbus::Result<String>;

    /// NetworkManager version and capabilities.
    #[zbus(property)]
    fn version_info(&self) -> zbus::Result<Vec<u32>>;

    /// The current set of capabilities.
    #[zbus(property)]
    fn capabilities(&self) -> zbus::Result<Vec<u32>>;

    /// The overall state of the NetworkManager daemon.
    #[zbus(property)]
    fn state(&self) -> zbus::Result<State>;

    /// The result of the last connectivity check.
    #[zbus(property)]
    fn connectivity(&self) -> zbus::Result<ConnectivityState>;

    /// Indicates whether connectivity checking service has been configured.
    #[zbus(property)]
    fn connectivity_check_available(&self) -> zbus::Result<bool>;

    /// Indicates whether connectivity checking is enabled.
    #[zbus(property)]
    fn connectivity_check_enabled(&self) -> zbus::Result<bool>;

    /// Set whether connectivity checking is enabled.
    #[zbus(property)]
    fn set_connectivity_check_enabled(&self, value: bool) -> zbus::Result<()>;

    /// The URI that NetworkManager will hit to check if there is internet
    /// connectivity.
    #[zbus(property)]
    fn connectivity_check_uri(&self) -> zbus::Result<String>;

    /// Dictionary of global DNS settings.
    #[zbus(property)]
    fn global_dns_configuration(&self) -> zbus::Result<HashMap<String, zvariant::OwnedValue>>;

    /// Set global DNS configuration.
    #[zbus(property)]
    fn set_global_dns_configuration(
        &self,
        value: HashMap<String, zvariant::Value<'_>>,
    ) -> zbus::Result<()>;
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
    fn active_access_point(&self) -> zbus::Result<zvariant::OwnedObjectPath>;

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
    fn devices(&self) -> zbus::Result<Vec<zvariant::OwnedObjectPath>>;

    /// The state of this active connection.
    #[zbus(property)]
    fn state(&self) -> zbus::Result<u32>;

    /// The path of the connection object.
    #[zbus(property)]
    fn connection(&self) -> zbus::Result<zvariant::OwnedObjectPath>;
}
