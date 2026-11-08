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
    /// Without a password, `saved_connection` is activated as-is. With a
    /// password, any `saved_connection` is replaced by a new profile using
    /// it, since its stored secrets were evidently rejected. `security`
    /// determines what key-mgmt scheme to configure for the password.
    Connect {
        ssid: String,
        security: ApSecurity,
        password: Option<String>,
        saved_connection: Option<OwnedObjectPath>,
    },
    /// Disconnects the wifi device.
    Disconnect,
    /// Deletes a saved connection profile.
    Forget(OwnedObjectPath),
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
/// Pass the network's `saved_connection` (if any) and no password to
/// activate the saved profile. Pass a password to create a new profile with
/// it, replacing `saved_connection` if one is given.
pub fn connect(
    ssid: String,
    security: ApSecurity,
    password: Option<String>,
    saved_connection: Option<OwnedObjectPath>,
) {
    send(NetworkCommand::Connect {
        ssid,
        security,
        password,
        saved_connection,
    });
}

/// Disconnects from the current wifi network.
pub fn disconnect() {
    send(NetworkCommand::Disconnect);
}

/// Deletes a saved connection profile.
pub fn forget(connection: OwnedObjectPath) {
    send(NetworkCommand::Forget(connection));
}
