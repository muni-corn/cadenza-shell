//! UPower proxies for locating the battery's device on the system bus.
//!
//! UPower already watches udev and re-reads batteries, then announces the
//! result as D-Bus property changes, which is what the battery watcher uses to
//! refresh promptly.

use std::{ffi::OsStr, path::Path};

use zbus::{proxy, zvariant::OwnedObjectPath};

mod trigger;

pub use trigger::run_upower_trigger;

#[proxy(
    interface = "org.freedesktop.UPower",
    default_service = "org.freedesktop.UPower",
    default_path = "/org/freedesktop/UPower"
)]
trait UPower {
    /// Lists the object paths of every power device UPower knows about.
    fn enumerate_devices(&self) -> zbus::Result<Vec<OwnedObjectPath>>;
}

#[proxy(
    interface = "org.freedesktop.UPower.Device",
    default_service = "org.freedesktop.UPower"
)]
trait UPowerDevice {
    /// The sysfs path or name the device was created from, such as `BAT0`.
    #[zbus(property)]
    fn native_path(&self) -> zbus::Result<String>;
}

/// Finds UPower's object path for the sysfs battery called `battery_name`.
///
/// Matching by name keeps peripheral batteries (mice, controllers) from
/// triggering updates.
///
/// # Errors
/// Fails if UPower can't be reached or doesn't list a matching device.
pub async fn find_battery_device(
    conn: &zbus::Connection,
    battery_name: &OsStr,
) -> zbus::Result<OwnedObjectPath> {
    let upower = UPowerProxy::new(conn).await?;

    for path in upower.enumerate_devices().await? {
        let device = UPowerDeviceProxy::builder(conn)
            .path(&path)?
            .build()
            .await?;

        // a device that vanishes mid-enumeration shouldn't fail the lookup
        let Ok(native_path) = device.native_path().await else {
            continue;
        };

        if Path::new(&native_path).file_name() == Some(battery_name) {
            return Ok(path);
        }
    }

    Err(zbus::Error::Failure(format!(
        "upower doesn't list a device for battery {battery_name:?}"
    )))
}
