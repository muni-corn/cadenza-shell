use std::{fs, path::Path};

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
    /// Current charge in microampere-hours (µAh).
    pub charge_now: Option<u64>,
    /// Current draw in microamperes (µA). Positive values indicate charging,
    /// negative values indicate discharging.
    pub current_now: Option<i64>,
    /// Voltage in microvolts (µV).
    pub voltage_now: Option<u64>,
    /// Full charge capacity in microampere-hours (µAh).
    pub charge_full: Option<u64>,
    /// Design capacity in microampere-hours (µAh).
    pub charge_full_design: Option<u64>,
    /// Charging status.
    pub status: ChargingStatus,
}

impl SysfsReading {
    /// Calculate current power draw in watts.
    /// Returns None if voltage_now or current_now are unavailable.
    pub fn power_watts(&self) -> Option<f64> {
        let voltage = self.voltage_now?;
        let current = self.current_now?.unsigned_abs();

        // convert µV × µA = pW (picowatts), then to watts
        let power_picowatts = voltage as f64 * current as f64;
        Some(power_picowatts / 1_000_000_000_000.0)
    }

    /// Calculate battery health as percentage of design capacity.
    /// Returns None if either capacity value is unavailable.
    pub fn battery_health(&self) -> Option<f64> {
        let full = self.charge_full? as f64;
        let design = self.charge_full_design? as f64;

        if design > 0.0 {
            Some((full / design).clamp(0.0, 1.0))
        } else {
            None
        }
    }

    /// Calculate precise percentage remaining.
    /// Returns None if charge values are unavailable.
    pub fn percentage(&self) -> Option<f64> {
        let now = self.charge_now? as f64;
        let full = self.charge_full? as f64;

        if full > 0.0 {
            Some((now / full).clamp(0.0, 1.0))
        } else {
            None
        }
    }
}

/// Read battery data from sysfs.
/// Returns None if sysfs interface is unavailable or current_now cannot be
/// read.
pub fn read_battery_sysfs() -> Option<SysfsReading> {
    let battery_path = detect_battery_path()?;

    // current_now is critical for power calculation - if unavailable, return None
    let current_now = read_sysfs_i64(&battery_path, "current_now")?;

    // read other values, allowing them to be missing
    let charge_now = read_sysfs_u64(&battery_path, "charge_now");
    let voltage_now = read_sysfs_u64(&battery_path, "voltage_now");
    let charge_full = read_sysfs_u64(&battery_path, "charge_full");
    let charge_full_design = read_sysfs_u64(&battery_path, "charge_full_design");

    let status = read_charging_status(&battery_path);

    Some(SysfsReading {
        charge_now,
        current_now: Some(current_now),
        voltage_now,
        charge_full,
        charge_full_design,
        status,
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
            charge_now: Some(5_000_000),   // 5 Ah
            current_now: Some(1_500_000),  // 1.5 A
            voltage_now: Some(12_000_000), // 12 V
            charge_full: Some(6_000_000),
            charge_full_design: Some(6_500_000),
            status: ChargingStatus::Discharging,
        };

        // 12V × 1.5A = 18W
        let power = reading.power_watts().unwrap();
        assert!((power - 18.0).abs() < 0.01);
    }

    #[test]
    fn test_battery_health() {
        let reading = SysfsReading {
            charge_now: None,
            current_now: None,
            voltage_now: None,
            charge_full: Some(5_200_000),        // 5.2 Ah
            charge_full_design: Some(6_500_000), // 6.5 Ah
            status: ChargingStatus::Full,
        };

        // 5.2 / 6.5 = 0.8
        let health = reading.battery_health().unwrap();
        assert!((health - 0.8).abs() < 0.01);
    }

    #[test]
    fn test_percentage() {
        let reading = SysfsReading {
            charge_now: Some(3_250_000), // 3.25 Ah
            current_now: None,
            voltage_now: None,
            charge_full: Some(6_500_000), // 6.5 Ah
            charge_full_design: None,
            status: ChargingStatus::Discharging,
        };

        // 3.25 / 6.5 = 0.5
        let percentage = reading.percentage().unwrap();
        assert!((percentage - 0.5).abs() < 0.01);
    }
}
