use std::{fs, path::Path};

#[derive(Clone, Copy, Debug)]
pub enum BatteryCapacity {
    /// µAh
    MicroAmpereHours(u64),

    /// µWh
    MicroWattHours(u64),
}

impl BatteryCapacity {
    pub fn div(self, rhs: Self) -> Option<f64> {
        match (self, rhs) {
            (Self::MicroAmpereHours(l), Self::MicroAmpereHours(r))
            | (Self::MicroWattHours(l), Self::MicroWattHours(r)) => Some(l as f64 / r as f64),
            _ => None,
        }
    }
}

/// Charging status of the battery.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ChargingStatus {
    Charging,
    Discharging,
    Full,
    NotCharging,
}

/// Raw reading from sysfs battery interface.
#[derive(Debug, Clone)]
pub struct SysfsReading {
    /// Current voltage in microvolts (µV).
    pub voltage_now: u64,

    /// Current draw in microamperes (µA). Positive values indicate charging,
    /// negative values indicate discharging.
    pub current_now: i64,

    /// Current capacity in either milliwatt-hours (µWh) or milliampere-hours
    /// (µAh).
    pub capacity_now: BatteryCapacity,

    /// Full charge capacity in microampere-hours (µAh).
    pub capacity_full: BatteryCapacity,

    /// Charging status.
    pub status: ChargingStatus,
}

impl SysfsReading {
    /// Calculate current power draw in watts.
    /// Returns None if voltage_now or current_now are unavailable.
    pub fn power_watts(&self) -> Option<f64> {
        let voltage = self.voltage_now;
        let current = self.current_now.unsigned_abs();

        // convert µV × µA = pW (picowatts), then to watts
        let power_picowatts = voltage as f64 * current as f64;
        Some(power_picowatts / 1_000_000_000_000.0)
    }

    /// Calculate precise percentage remaining.
    /// Returns None if charge values are unavailable or incompatible.
    pub fn percentage(&self) -> Option<f64> {
        self.capacity_now
            .div(self.capacity_full)
            .map(|p| p.clamp(0.0, 1.0))
    }
}

/// Read battery data from sysfs.
/// Returns None if sysfs interface is unavailable or current_now cannot be
/// read.
pub fn read_battery_sysfs() -> Option<SysfsReading> {
    let battery_path = detect_battery_path()?;

    // current_now and voltage_now are critical for power calculation - if
    // unavailable, return None
    let current_now = read_sysfs_i64(&battery_path, "current_now")?;
    let voltage_now = read_sysfs_u64(&battery_path, "voltage_now")?;

    // read other values, allowing them to be missing
    let charge_now =
        read_sysfs_u64(&battery_path, "charge_now").map(BatteryCapacity::MicroAmpereHours);
    let charge_full =
        read_sysfs_u64(&battery_path, "charge_full").map(BatteryCapacity::MicroAmpereHours);
    let energy_now =
        read_sysfs_u64(&battery_path, "energy_now").map(BatteryCapacity::MicroWattHours);
    let energy_full =
        read_sysfs_u64(&battery_path, "energy_full").map(BatteryCapacity::MicroWattHours);

    let status = read_charging_status(&battery_path);

    Some(SysfsReading {
        voltage_now,
        current_now,
        status,

        // prefer watt-hours over amp-hours. if we only have amp-hours, we can calculate watt-hours
        // with our voltage.
        capacity_now: energy_now.or(charge_now)?,
        capacity_full: energy_full.or(charge_full)?,
    })
}

/// Detect the battery sysfs path.
fn detect_battery_path() -> Option<String> {
    let power_supply_path = Path::new("/sys/class/power_supply");

    fs::read_dir(power_supply_path).ok()?.find_map(|entry| {
        let entry = entry.ok()?;
        let path = entry.path();

        if path.is_dir() {
            let type_path = path.join("type");
            if type_path.exists() {
                let type_content = fs::read_to_string(&type_path).ok()?;
                if type_content.trim().eq_ignore_ascii_case("battery") {
                    Some(path.to_string_lossy().to_string())
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
fn read_sysfs_u64(battery_path: &str, file: &str) -> Option<u64> {
    let path = Path::new(battery_path).join(file);
    fs::read_to_string(path).ok()?.trim().parse().ok()
}

/// Read a sysfs file as i64 (for current_now which can be negative).
fn read_sysfs_i64(battery_path: &str, file: &str) -> Option<i64> {
    let path = Path::new(battery_path).join(file);
    fs::read_to_string(path).ok()?.trim().parse().ok()
}

/// Read charging status from sysfs.
fn read_charging_status(battery_path: &str) -> ChargingStatus {
    let path = Path::new(battery_path).join("status");
    fs::read_to_string(path)
        .ok()
        .and_then(|s| match s.trim() {
            "Charging" => Some(ChargingStatus::Charging),
            "Discharging" => Some(ChargingStatus::Discharging),
            "Full" => Some(ChargingStatus::Full),
            "Not charging" => Some(ChargingStatus::NotCharging),
            _ => None,
        })
        .unwrap_or(ChargingStatus::Discharging) // safe default
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_power_calculation() {
        let reading = SysfsReading {
            current_now: 1_500_000,                                      // 1.5 A
            voltage_now: 12_000_000,                                     // 12 V
            capacity_now: BatteryCapacity::MicroAmpereHours(5_000_000),  // 5 Ah
            capacity_full: BatteryCapacity::MicroAmpereHours(6_000_000), // 6 Ah
            status: ChargingStatus::Discharging,
        };

        // 12V × 1.5A = 18W
        let power = reading.power_watts().unwrap();
        assert!((power - 18.0).abs() < 0.01);
    }

    #[test]
    fn test_percentage() {
        let reading = SysfsReading {
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
