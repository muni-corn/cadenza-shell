use gtk4::{glib, subclass::prelude::*};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BatteryStatus {
    Unknown,
    Charging,
    Discharging,
    NotCharging,
    Full,
}

impl Default for BatteryStatus {
    fn default() -> Self {
        Self::Unknown
    }
}

mod imp {
    use std::{
        cell::{Cell, RefCell},
        fs,
        path::Path,
    };

    use anyhow::Result;
    use gtk4::{glib, prelude::*, subclass::prelude::*};

    use super::BatteryStatus;

    #[derive(glib::Properties, Default)]
    #[properties(wrapper_type = super::BatteryService)]
    pub struct BatteryService {
        #[property(get, set, minimum = 0.0, maximum = 1.0)]
        percentage: Cell<f64>,

        #[property(get, set)]
        available: Cell<bool>,

        #[property(get, set)]
        charging: Cell<bool>,

        #[property(get, set)]
        time_remaining: Cell<i32>, // minutes, -1 if unknown

        battery_path: RefCell<String>,
        status: RefCell<BatteryStatus>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for BatteryService {
        type ParentType = glib::Object;
        type Type = super::BatteryService;

        const NAME: &'static str = "MuseShellBatteryService";
    }

    #[glib::derived_properties]
    impl ObjectImpl for BatteryService {
        fn constructed(&self) {
            self.parent_constructed();

            // Initialize battery monitoring
            if let Ok(battery_path) = self.detect_battery() {
                self.battery_path.replace(battery_path);
                self.available.set(true);

                // Start monitoring
                self.start_monitoring();

                // Initial state update
                if let Ok((percentage, status, time_remaining)) = self.read_battery_state() {
                    self.percentage.set(percentage);
                    self.status.replace(status);
                    self.charging.set(matches!(status, BatteryStatus::Charging));
                    self.time_remaining.set(time_remaining);
                }
            } else {
                log::warn!("No battery detected, battery service unavailable");
                self.available.set(false);
            }
        }
    }

    impl BatteryService {
        fn detect_battery(&self) -> Result<String> {
            let power_supply_path = Path::new("/sys/class/power_supply");
            let entries = fs::read_dir(power_supply_path)?;

            for entry in entries {
                let entry = entry?;
                let path = entry.path();

                // Check if this is a battery (not AC adapter)
                if let Ok(type_content) = fs::read_to_string(path.join("type")) {
                    if type_content.trim() == "Battery" {
                        return Ok(path.to_string_lossy().to_string());
                    }
                }
            }

            anyhow::bail!("No battery found")
        }

        fn read_battery_state(&self) -> Result<(f64, BatteryStatus, i32)> {
            let battery_path = self.battery_path.borrow();
            let base_path = Path::new(&*battery_path);

            // Read capacity (percentage)
            let capacity_path = base_path.join("capacity");
            let capacity_str = fs::read_to_string(capacity_path)?;
            let percentage = capacity_str.trim().parse::<f64>()? / 100.0;

            // Read status
            let status_path = base_path.join("status");
            let status_str = fs::read_to_string(status_path)?;
            let status = match status_str.trim() {
                "Charging" => BatteryStatus::Charging,
                "Discharging" => BatteryStatus::Discharging,
                "Not charging" => BatteryStatus::NotCharging,
                "Full" => BatteryStatus::Full,
                _ => BatteryStatus::Unknown,
            };

            // Try to calculate time remaining
            let time_remaining = self.calculate_time_remaining(base_path, &status, percentage)?;

            Ok((percentage, status, time_remaining))
        }

        fn calculate_time_remaining(
            &self,
            base_path: &Path,
            status: &BatteryStatus,
            percentage: f64,
        ) -> Result<i32> {
            // Try to read power_now and energy_now for more accurate calculation
            let power_now_path = base_path.join("power_now");
            let energy_now_path = base_path.join("energy_now");
            let energy_full_path = base_path.join("energy_full");

            if let (Ok(power_str), Ok(energy_str), Ok(energy_full_str)) = (
                fs::read_to_string(power_now_path),
                fs::read_to_string(energy_now_path),
                fs::read_to_string(energy_full_path),
            ) {
                if let (Ok(power_now), Ok(energy_now), Ok(energy_full)) = (
                    power_str.trim().parse::<f64>(),
                    energy_str.trim().parse::<f64>(),
                    energy_full_str.trim().parse::<f64>(),
                ) {
                    if power_now > 0.0 {
                        let time_hours = match status {
                            BatteryStatus::Charging => (energy_full - energy_now) / power_now,
                            BatteryStatus::Discharging => energy_now / power_now,
                            _ => return Ok(-1),
                        };
                        return Ok((time_hours * 60.0) as i32);
                    }
                }
            }

            // Fallback: simple estimation based on percentage
            match status {
                BatteryStatus::Discharging => {
                    // Rough estimate: assume 8 hours at 100%
                    Ok((percentage * 8.0 * 60.0) as i32)
                }
                BatteryStatus::Charging => {
                    // Rough estimate: assume 2 hours to charge from 0% to 100%
                    Ok(((1.0 - percentage) * 2.0 * 60.0) as i32)
                }
                _ => Ok(-1),
            }
        }

        fn start_monitoring(&self) {
            let obj = self.obj().clone();

            // Monitor battery changes every 10 seconds
            glib::timeout_add_local(std::time::Duration::from_secs(10), move || {
                if let Ok((percentage, status, time_remaining)) = obj.imp().read_battery_state() {
                    // Only update if values changed to avoid unnecessary signals
                    if (obj.percentage() - percentage).abs() > 0.01 {
                        obj.set_percentage(percentage);
                    }

                    let is_charging = matches!(status, BatteryStatus::Charging);
                    if obj.charging() != is_charging {
                        obj.set_charging(is_charging);
                    }

                    if obj.time_remaining() != time_remaining {
                        obj.set_time_remaining(time_remaining);
                    }

                    obj.imp().status.replace(status);
                }
                glib::ControlFlow::Continue
            });
        }

        pub fn get_status(&self) -> BatteryStatus {
            *self.status.borrow()
        }
    }
}

glib::wrapper! {
    pub struct BatteryService(ObjectSubclass<imp::BatteryService>);
}

impl Default for BatteryService {
    fn default() -> Self {
        Self::new()
    }
}

impl BatteryService {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    pub fn status(&self) -> BatteryStatus {
        self.imp().get_status()
    }

    pub fn is_low(&self) -> bool {
        self.percentage() < 0.15 && !self.charging()
    }

    pub fn is_critical(&self) -> bool {
        self.percentage() < 0.05 && !self.charging()
    }

    pub fn time_remaining_formatted(&self) -> String {
        let minutes = self.time_remaining();
        if minutes < 0 {
            return "Unknown".to_string();
        }

        let hours = minutes / 60;
        let mins = minutes % 60;

        if hours > 0 {
            format!("{}h {}m", hours, mins)
        } else {
            format!("{}m", mins)
        }
    }
}
