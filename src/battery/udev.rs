//! Udev monitoring for battery status changes.
//!
//! This module provides helpers for creating a udev monitor on the
//! `power_supply` subsystem and extracting charging status from events.

use udev::{Enumerator, MonitorSocket};

use crate::battery::ChargingStatus;

/// Creates a udev monitor socket filtered to the `power_supply` subsystem.
pub fn create_battery_monitor() -> anyhow::Result<MonitorSocket> {
    log::debug!("getting power_supply devices for monitor...");

    let mut enumerator = Enumerator::new()?;
    enumerator.match_subsystem("power_supply")?;
    for dev in enumerator.scan_devices()? {
        log::debug!("found power_supply device: {dev:?}")
    }

    let socket = udev::MonitorBuilder::new()?
        .match_subsystem("power_supply")?
        .listen()?;

    log::debug!("done. returning socket");
    Ok(socket)
}

/// Returns true if the event is a `change` action on a battery device.
pub fn is_battery_change(event: &udev::Event) -> bool {
    let is_change = event.action().is_some_and(|a| a.to_str() == Some("change"));

    let is_battery = event.property_value("POWER_SUPPLY_TYPE").is_some_and(|v| {
        v.to_str()
            .is_some_and(|s| s.eq_ignore_ascii_case("battery"))
    });

    is_change && is_battery
}

/// Reads and parses the charging status from a udev event's
/// `POWER_SUPPLY_STATUS` property.
pub fn read_status_from_event(event: &udev::Event) -> Option<ChargingStatus> {
    let value = event.property_value("POWER_SUPPLY_STATUS")?;
    Some(parse_charging_status(value.to_str()?))
}

/// Parses a charging status string into a [`ChargingStatus`] variant.
pub fn parse_charging_status(value: &str) -> ChargingStatus {
    match value.trim() {
        "Charging" => ChargingStatus::Charging,
        "Discharging" => ChargingStatus::Discharging,
        "Full" => ChargingStatus::Full,
        "Not charging" => ChargingStatus::NotCharging,
        _ => ChargingStatus::Unknown,
    }
}
