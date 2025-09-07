use std::{
    cell::{Cell, RefCell},
    fs,
    path::Path,
};

use anyhow::Result;
use gtk4::{glib, prelude::*, subclass::prelude::*};

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum BatteryStatus {
    #[default]
    Unknown,
    Charging,
    Discharging,
    NotCharging,
    Full,
}

mod imp {
    use super::*;

    #[derive(glib::Properties, Default)]
    #[properties(wrapper_type = super::BatteryService)]
    pub struct BatteryService {
        #[property(get, set, minimum = 0.0, maximum = 1.0)]
        percentage: Cell<f64>,

        #[property(get, set)]
        available: Cell<bool>,

        #[property(get, set)]
        charging: Cell<bool>,

        battery_path: RefCell<Option<String>>,
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

            // initialize battery monitoring
            if let Ok(battery_path) = detect_battery() {
                self.battery_path.replace(Some(battery_path));
                self.available.set(true);

                // start monitoring
                self.start_monitoring();

                // initial state update
                if let Ok((percentage, status)) = self.read_battery_state() {
                    self.percentage.set(percentage);
                    self.status.replace(status);
                    self.charging.set(matches!(status, BatteryStatus::Charging));
                }
            } else {
                log::warn!("no battery detected, battery service unavailable");
                self.available.set(false);
            }
        }
    }

    impl BatteryService {
        fn read_battery_state(&self) -> Result<(f64, BatteryStatus)> {
            let battery_path_guard = self.battery_path.borrow();
            let Some(ref battery_path) = *battery_path_guard else {
                return Ok((0.0, BatteryStatus::Unknown));
            };

            let base_path = Path::new(battery_path);

            // read capacity (percentage)
            let capacity_path = base_path.join("capacity");
            let capacity_str = fs::read_to_string(capacity_path)?;
            let percentage = capacity_str.trim().parse::<f64>()? / 100.0;

            // read status
            let status_path = base_path.join("status");
            let status_str = fs::read_to_string(status_path)?;
            let status = match status_str.trim() {
                "Charging" => BatteryStatus::Charging,
                "Discharging" => BatteryStatus::Discharging,
                "Not charging" => BatteryStatus::NotCharging,
                "Full" => BatteryStatus::Full,
                _ => BatteryStatus::Unknown,
            };

            Ok((percentage, status))
        }

        fn start_monitoring(&self) {
            let obj = self.obj().clone();

            // monitor battery changes every 10 seconds
            glib::timeout_add_local(std::time::Duration::from_secs(10), move || {
                if let Ok((percentage, status)) = obj.imp().read_battery_state() {
                    // only update if values changed to avoid unnecessary signals
                    if (obj.percentage() - percentage).abs() > 0.01 {
                        obj.set_percentage(percentage);
                    }

                    let is_charging = matches!(status, BatteryStatus::Charging);
                    if obj.charging() != is_charging {
                        obj.set_charging(is_charging);
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

fn detect_battery() -> Result<String> {
    let power_supply_path = Path::new("/sys/class/power_supply");
    let entries = fs::read_dir(power_supply_path)?;

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        // check if this is a battery (not AC adapter)
        if let Ok(type_content) = fs::read_to_string(path.join("type"))
            && type_content.trim() == "Battery"
        {
            return Ok(path.to_string_lossy().to_string());
        }
    }

    anyhow::bail!("no battery found")
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
}
