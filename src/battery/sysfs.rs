use std::{
    fs,
    path::{Path, PathBuf},
};

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

use crate::battery::{BatteryCapacity, ChargingStatus};

/// Raw reading from sysfs battery interface.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SysfsReading {
    /// The time of the reading.
    pub when: DateTime<Local>,

    /// Current voltage in microvolts (µV).
    pub voltage_now: u64,

    /// Current draw in microamperes (µA).
    pub current_now: i64,

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
    /// Calculate current power draw in watts.
    pub fn power_watts(&self) -> f64 {
        let voltage = self.voltage_now;
        let current = self.current_now.unsigned_abs();

        // convert µV × µA = pW (picowatts), then to watts
        let power_picowatts = voltage as f64 * current as f64;
        power_picowatts / 1_000_000_000_000.0
    }

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
        self.capacity_now.div(self.capacity_full)
    }
}

/// Read battery data from sysfs at the given battery path.
///
/// Returns `None` if `current_now` or `voltage_now` are unavailable, as these
/// are critical for power calculation.
pub fn read_battery_sysfs(battery_path: &Path) -> Option<SysfsReading> {
    // current_now and voltage_now are critical for power calculation - if
    // unavailable, return None
    let current_now = read_sysfs_i64(battery_path, "current_now")?;
    let voltage_now = read_sysfs_u64(battery_path, "voltage_now")?;

    // read other values, allowing them to be missing
    let charge_now =
        read_sysfs_u64(battery_path, "charge_now").map(BatteryCapacity::MicroAmpereHours);
    let charge_full =
        read_sysfs_u64(battery_path, "charge_full").map(BatteryCapacity::MicroAmpereHours);
    let energy_now =
        read_sysfs_u64(battery_path, "energy_now").map(BatteryCapacity::MicroWattHours);
    let energy_full =
        read_sysfs_u64(battery_path, "energy_full").map(BatteryCapacity::MicroWattHours);

    let status = read_charging_status(battery_path);

    Some(SysfsReading {
        when: Local::now(),
        voltage_now,
        current_now,
        status,

        // prefer watt-hours over amp-hours. if we only have amp-hours, we can calculate watt-hours
        // with our voltage.
        capacity_now: energy_now.or(charge_now)?,
        capacity_full: energy_full.or(charge_full)?,
    })
}

/// Stable identity for a battery device, used to key per-device learned
/// parameters across reboots.
#[derive(Debug, Clone)]
pub struct BatteryIdentity {
    /// Value of the sysfs `serial_number` file, if present.
    pub serial_number: Option<String>,
    /// Value of the sysfs `model_name` file, if present.
    pub model_name: Option<String>,
    /// Value of the sysfs `manufacturer` file, if present.
    pub manufacturer: Option<String>,
    /// The last component of the sysfs path (e.g. `"BAT0"`).
    pub sysfs_name: String,
}

impl BatteryIdentity {
    /// Build a stable, filename-safe key for this device.
    ///
    /// Preference order:
    /// 1. `serial_number` (most unique)
    /// 2. `manufacturer + "_" + model_name`
    /// 3. `sysfs_name` (least stable across battery replacements)
    pub fn device_key(&self) -> String {
        let sanitize = |s: &str| {
            s.trim()
                .chars()
                .map(|c| {
                    if c.is_alphanumeric() || c == '-' {
                        c
                    } else {
                        '_'
                    }
                })
                .collect::<String>()
        };

        if let Some(ref sn) = self.serial_number {
            let key = sanitize(sn);
            if !key.is_empty() {
                return key;
            }
        }

        match (&self.manufacturer, &self.model_name) {
            (Some(mfr), Some(mdl)) => {
                let key = format!("{}_{}", sanitize(mfr), sanitize(mdl));
                if key.len() > 1 {
                    return key;
                }
            }
            (None, Some(mdl)) => {
                let key = sanitize(mdl);
                if !key.is_empty() {
                    return key;
                }
            }
            _ => {}
        }

        sanitize(&self.sysfs_name)
    }
}

/// Read battery identity fields from the sysfs path.
pub fn read_battery_identity(battery_path: &Path) -> BatteryIdentity {
    let read_opt = |file: &str| {
        fs::read_to_string(battery_path.join(file))
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    };

    let sysfs_name = battery_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "battery".to_string());

    BatteryIdentity {
        serial_number: read_opt("serial_number"),
        model_name: read_opt("model_name"),
        manufacturer: read_opt("manufacturer"),
        sysfs_name,
    }
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
    fn test_power_calculation() {
        let reading = SysfsReading {
            when: Local::now(),
            current_now: 1_500_000,  // 1.5 A
            voltage_now: 12_000_000, // 12 V
            capacity_now: BatteryCapacity::MicroAmpereHours(5_000_000), // 5 Ah
            capacity_full: BatteryCapacity::MicroAmpereHours(6_000_000), // 6 Ah
            status: ChargingStatus::Discharging,
        };

        // 12V × 1.5A = 18W
        let power = reading.power_watts();
        assert!((power - 18.0).abs() < 0.01);
    }

    #[test]
    fn test_percentage() {
        let reading = SysfsReading {
            when: Local::now(),
            current_now: 1_000_000,
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
