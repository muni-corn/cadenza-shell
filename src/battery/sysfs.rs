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
        match self.capacity_now {
            BatteryCapacity::MicroWattHours(uwh) => uwh as f64 / 1_000_000.0,
            BatteryCapacity::MicroAmpereHours(uah) => {
                // convert µV × µAh = pWh (picowatt-hours), then to watt-hours
                let capacity_pwh = self.voltage_now * uah;
                capacity_pwh as f64 / 1e12_f64
            }
        }
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

/// Detect the battery sysfs path by scanning `/sys/class/power_supply/` for
/// devices with type "Battery".
pub fn detect_battery_path() -> Option<PathBuf> {
    let power_supply_path = Path::new("/sys/class/power_supply");

    fs::read_dir(power_supply_path).ok()?.find_map(|entry| {
        let entry = entry.ok()?;
        let path = entry.path();

        if path.is_dir() {
            let type_path = path.join("type");
            if type_path.exists() {
                let type_content = fs::read_to_string(&type_path).ok()?;
                if type_content.trim().eq_ignore_ascii_case("battery") {
                    Some(path)
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        }
    })
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
