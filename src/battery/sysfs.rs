use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::battery::{BatteryCapacity, ChargingStatus};

/// Raw reading from sysfs battery interface.
#[derive(Debug, Clone)]
pub struct SysfsReading {
    /// Current voltage in microvolts (µV).
    pub voltage_now: u64,

    /// Current power draw in watts, from `power_now` when the battery
    /// reports it, otherwise from voltage times `current_now`.
    pub power_watts: f64,

    /// Current capacity in either milliwatt-hours (µWh) or milliampere-hours
    /// (µAh).
    pub capacity_now: BatteryCapacity,

    /// Full charge capacity in either milliwatt-hours (µWh) or
    /// milliampere-hours (µAh).
    pub capacity_full: BatteryCapacity,

    /// Charging status.
    pub status: ChargingStatus,
}

impl SysfsReading {
    /// Returns the remaining capacity in this reading, in Wh.
    pub fn remaining_wh(&self) -> f64 {
        self.capacity_now.as_wh(self.voltage_now)
    }

    /// Returns the total capacity of this reading, in Wh.
    pub fn capacity_wh(&self) -> f64 {
        self.capacity_full.as_wh(self.voltage_now)
    }

    /// Calculate precise percentage remaining.
    /// Returns None if charge values are unavailable or incompatible.
    pub fn percentage(&self) -> Option<f64> {
        self.capacity_now.ratio(self.capacity_full)
    }
}

/// Read battery data from sysfs at the given battery path.
///
/// Returns `None` if the voltage or the power draw (`power_now`, or
/// `current_now` to derive it) is unavailable, as these are critical for
/// power calculation.
pub fn read_battery_sysfs(battery_path: &Path) -> Option<SysfsReading> {
    let voltage_now = read_sysfs_u64(battery_path, "voltage_now")?;

    // energy-reporting batteries (most thinkpads and amd laptops) expose
    // power_now in µW and often no current_now at all
    let power_watts = match read_sysfs_u64(battery_path, "power_now") {
        Some(power_uw) => power_uw as f64 / 1_000_000.0,
        None => {
            let current_ua = read_sysfs_i64(battery_path, "current_now")?.unsigned_abs();
            // µV × µA = pW
            voltage_now as f64 * current_ua as f64 / 1_000_000_000_000.0
        }
    };

    // prefer watt-hours over amp-hours, but always take "now" and "full" in
    // the same unit, since a mixed pair has no meaningful ratio
    let energy = read_sysfs_u64(battery_path, "energy_now")
        .zip(read_sysfs_u64(battery_path, "energy_full"))
        .map(|(now, full)| {
            (
                BatteryCapacity::MicroWattHours(now),
                BatteryCapacity::MicroWattHours(full),
            )
        });
    let charge = || {
        read_sysfs_u64(battery_path, "charge_now")
            .zip(read_sysfs_u64(battery_path, "charge_full"))
            .map(|(now, full)| {
                (
                    BatteryCapacity::MicroAmpereHours(now),
                    BatteryCapacity::MicroAmpereHours(full),
                )
            })
    };
    let (capacity_now, capacity_full) = energy.or_else(charge)?;

    Some(SysfsReading {
        voltage_now,
        power_watts,
        status: read_charging_status(battery_path),
        capacity_now,
        capacity_full,
    })
}

/// Detect the system battery by scanning `/sys/class/power_supply/` for
/// devices with type "Battery".
///
/// Mice, controllers and other peripherals also report type "Battery", but
/// with scope "Device", so those are skipped.
pub fn detect_battery_path() -> Option<PathBuf> {
    let power_supply_path = Path::new("/sys/class/power_supply");

    let candidates: Vec<String> = fs::read_dir(power_supply_path)
        .ok()?
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            let read = |file: &str| fs::read_to_string(path.join(file)).unwrap_or_default();

            let is_battery = read("type").trim().eq_ignore_ascii_case("battery");
            let is_peripheral = read("scope").trim().eq_ignore_ascii_case("device");
            (is_battery && !is_peripheral)
                .then(|| path.file_name()?.to_str().map(str::to_owned))
                .flatten()
        })
        .collect();

    pick_battery(&candidates).map(|name| power_supply_path.join(name))
}

/// Picks the system battery from candidate names, preferring the
/// conventional `BAT*` names and breaking ties alphabetically.
fn pick_battery(candidates: &[String]) -> Option<&str> {
    candidates
        .iter()
        .min_by_key(|name| (!name.starts_with("BAT"), name.as_str()))
        .map(String::as_str)
}

/// Read a sysfs file as u64.
fn read_sysfs_u64(battery_path: &Path, file: &str) -> Option<u64> {
    fs::read_to_string(battery_path.join(file))
        .ok()?
        .trim()
        .parse()
        .ok()
}

/// Read a sysfs file as i64 (for current_now which can be negative).
fn read_sysfs_i64(battery_path: &Path, file: &str) -> Option<i64> {
    fs::read_to_string(battery_path.join(file))
        .ok()?
        .trim()
        .parse()
        .ok()
}

/// Read charging status from sysfs, delegating to the shared parser in `udev`.
fn read_charging_status(battery_path: &Path) -> ChargingStatus {
    fs::read_to_string(battery_path.join("status"))
        .ok()
        .map(|s| crate::battery::udev::parse_charging_status(&s))
        .unwrap_or(ChargingStatus::Unknown)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_bat_named_batteries() {
        let candidates = vec!["CMB0".to_string(), "BAT1".to_string(), "BAT0".to_string()];
        assert_eq!(pick_battery(&candidates), Some("BAT0"));
    }

    #[test]
    fn falls_back_to_other_names() {
        let candidates = vec!["CMB0".to_string()];
        assert_eq!(pick_battery(&candidates), Some("CMB0"));
        assert_eq!(pick_battery(&[]), None);
    }

    #[test]
    fn remaining_wh_converts_charge_with_voltage() {
        let reading = SysfsReading {
            power_watts: 18.0,
            voltage_now: 12_000_000,
            capacity_now: BatteryCapacity::MicroAmpereHours(5_000_000),
            capacity_full: BatteryCapacity::MicroAmpereHours(6_000_000),
            status: ChargingStatus::Discharging,
        };

        // 12 V × 5 Ah = 60 Wh
        assert!((reading.remaining_wh() - 60.0).abs() < 0.01);
    }

    #[test]
    fn test_percentage() {
        let reading = SysfsReading {
            power_watts: 1.0,
            voltage_now: 1_000_000,
            capacity_now: BatteryCapacity::MicroAmpereHours(3_250_000), // 3.25 Ah
            capacity_full: BatteryCapacity::MicroAmpereHours(6_500_000), // 6.5 Ah
            status: ChargingStatus::Discharging,
        };

        // 3.25 / 6.5 = 0.5
        let percentage = reading.percentage().unwrap();
        assert!((percentage - 0.5).abs() < 0.01);
    }
}
