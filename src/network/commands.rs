use std::sync::OnceLock;

use tokio::sync::mpsc::UnboundedSender;
use zbus::zvariant::OwnedObjectPath;

use crate::network::types::ApSecurity;

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
    /// Connects to a wifi network by SSID. If NetworkManager already has a
    /// saved connection profile for this SSID, `password` is ignored and the
    /// saved secrets are used; otherwise a new profile is created. `security`
    /// determines what key-mgmt scheme to configure if `password` is given
    /// (ignored for `Open`/`Enterprise`, since those don't take an inline
    /// password).
    Connect {
        ssid: String,
        security: ApSecurity,
        password: Option<String>,
    },
    /// Deactivates the current primary connection, if any.
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

/// Requests a wifi scan. Has no effect if a scan is already in progress.
pub fn scan() {
    send(NetworkCommand::Scan);
}

/// Connects to a wifi network by SSID.
///
/// If NetworkManager already has a saved connection for this SSID,
/// `password` is ignored. Otherwise a new connection profile is created,
/// using `password` (and `security` to pick the key-mgmt scheme) if the
/// network requires one.
pub fn connect(ssid: String, security: ApSecurity, password: Option<String>) {
    send(NetworkCommand::Connect {
        ssid,
        security,
        password,
    });
}

/// Disconnects the currently active connection, if any.
pub fn disconnect() {
    send(NetworkCommand::Disconnect);
}

/// Deletes a saved connection profile.
pub fn forget(connection: OwnedObjectPath) {
    send(NetworkCommand::Forget(connection));
}
