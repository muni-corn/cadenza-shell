use std::sync::OnceLock;

use tokio::sync::mpsc::UnboundedSender;
use zbus::zvariant::OwnedObjectPath;

use crate::network::security::ApSecurity;

/// Commands the UI can send to the network service.
///
/// The service never touches D-Bus directly from UI code; every mutating
/// operation goes through this channel and is handled on the service's own
/// event loop, alongside the property-change and reconcile events it
/// already processes.
#[derive(Clone, Debug)]
pub(crate) enum NetworkCommand {
    SetWifiEnabled(bool),
    /// Requests a rescan. A no-op if a scan is already in flight.
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
        password: Option<String>,
        saved_connections: Vec<OwnedObjectPath>,
    },
    /// Disconnects the wifi device.
    Disconnect,
    /// Deletes saved connection profiles.
    Forget(Vec<OwnedObjectPath>),
}

static COMMAND_TX: OnceLock<UnboundedSender<NetworkCommand>> = OnceLock::new();

/// Installs the command sender. Called once from `run_network_service`.
///
/// Returns `Err` if called more than once (e.g. the service was somehow
/// started twice).
pub(crate) fn install_command_sender(tx: UnboundedSender<NetworkCommand>) -> Result<(), ()> {
    COMMAND_TX.set(tx).map_err(|_| ())
}

fn send(cmd: NetworkCommand) {
    match COMMAND_TX.get() {
        Some(tx) => {
            let _ = tx.send(cmd);
        }
        None => tracing::warn!("network command sent before the service started; dropping"),
    }
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
    password: Option<String>,
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
