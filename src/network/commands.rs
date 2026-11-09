use zbus::zvariant::OwnedObjectPath;

use crate::{
    network::{password::Password, security::ApSecurity},
    utils::command_slot::CommandSlot,
};

/// Commands the UI can send to the network service.
///
/// The service never touches D-Bus directly from UI code; every mutating
/// operation goes through this channel and is handled on the service's own
/// event loop, alongside the property-change and reconcile events it
/// already processes.
#[derive(Clone, Debug)]
pub(crate) enum NetworkCommand {
    SetWifiEnabled(bool),
    /// Requests a rescan. Skipped while the wifi radio is off.
    Scan,
    /// Connects to a wifi network by SSID.
    ///
    /// Without a password, the first of `saved_connections` is activated
    /// as-is. With a password, `saved_connections` are replaced by a new
    /// profile using it, since their stored secrets were evidently rejected.
    /// `security` determines what key-mgmt scheme to configure for the
    /// password.
    Connect {
        ssid: String,
        security: ApSecurity,
        password: Option<Password>,
        saved_connections: Vec<OwnedObjectPath>,
    },
    /// Disconnects the wifi device.
    Disconnect,
    /// Deletes saved connection profiles.
    Forget(Vec<OwnedObjectPath>),
}

/// Commands from the ui, received by the service loop.
pub(crate) static COMMANDS: CommandSlot<NetworkCommand> = CommandSlot::new("network");

fn send(cmd: NetworkCommand) {
    COMMANDS.send(cmd);
}

/// Enables or disables the wifi radio.
pub fn set_wifi_enabled(enabled: bool) {
    send(NetworkCommand::SetWifiEnabled(enabled));
}

/// Requests a wifi scan.
pub fn request_scan() {
    send(NetworkCommand::Scan);
}

/// Connects to a wifi network by SSID.
///
/// Pass the network's saved profiles and no password to activate a saved
/// profile. Pass a password to create a new profile with it, replacing any
/// saved profiles.
pub fn connect(
    ssid: String,
    security: ApSecurity,
    password: Option<Password>,
    saved_connections: Vec<OwnedObjectPath>,
) {
    send(NetworkCommand::Connect {
        ssid,
        security,
        password,
        saved_connections,
    });
}

/// Disconnects from the current wifi network.
pub fn disconnect() {
    send(NetworkCommand::Disconnect);
}

/// Deletes every saved profile of a network.
pub fn forget(connections: Vec<OwnedObjectPath>) {
    send(NetworkCommand::Forget(connections));
}
