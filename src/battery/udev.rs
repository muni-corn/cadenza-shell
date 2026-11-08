//! Udev monitoring for battery status changes.
//!
//! This module provides helpers for creating a udev monitor on the
//! `power_supply` subsystem and parsing charging status strings.

use udev::MonitorSocket;

use crate::battery::ChargingStatus;

/// Creates a udev monitor socket filtered to the `power_supply` subsystem.
pub fn create_battery_monitor() -> anyhow::Result<MonitorSocket> {
    let socket = udev::MonitorBuilder::new()?
        .match_subsystem("power_supply")?
        .listen()?;
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
