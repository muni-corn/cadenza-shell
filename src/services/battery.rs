use std::cell::{Cell, RefCell};

use chrono::Local;
use gtk4::{glib, prelude::*, subclass::prelude::*};
use systemstat::{Platform, System};

mod imp {
    use super::*;

    #[derive(glib::Properties)]
    #[properties(wrapper_type = super::BatteryService)]
    pub struct BatteryService {
        #[property(get, set, minimum = 0.0, maximum = 1.0)]
        percentage: Cell<f64>,

        #[property(get, set)]
        available: Cell<bool>,

        #[property(get, set)]
        charging: Cell<bool>,

        #[property(get, set)]
        time_to_empty: Cell<i32>, // seconds, -1 if unknown

        #[property(get, set)]
        time_to_full: Cell<i32>, // seconds, -1 if unknown

        system: RefCell<Option<System>>,
    }

    impl Default for BatteryService {
        fn default() -> Self {
            Self {
                percentage: Cell::new(0.0),
                available: Cell::new(false),
                charging: Cell::new(false),
                time_to_empty: Cell::new(-1),
                time_to_full: Cell::new(-1),
                system: RefCell::new(None),
            }
        }
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

            // initialize systemstat
            let system = System::new();
            self.system.replace(Some(system));

            // check if battery is available
            if self.has_battery() {
                self.available.set(true);

                // initial state update
                if let Some((charging, percentage, time_to_empty, time_to_full)) =
                    self.read_battery_state()
                {
                    self.percentage.set(percentage);
                    self.charging.set(charging);
                    self.time_to_empty.set(time_to_empty);
                    self.time_to_full.set(time_to_full);
                }

                // start monitoring
                self.start_monitoring();
            } else {
                log::warn!("no battery detected, battery service unavailable");
                self.available.set(false);
            }
        }
    }

    impl BatteryService {
        fn has_battery(&self) -> bool {
            let system_guard = self.system.borrow();
            let Some(ref system) = *system_guard else {
                return false;
            };

            system.battery_life().is_ok()
        }

        fn read_battery_state(&self) -> Option<(bool, f64, i32, i32)> {
            let system_guard = self.system.borrow();
            let system = system_guard.as_ref()?;

            let battery_life = system.battery_life().ok()?;

            // get percentage (0.0 to 1.0)
            let percentage = battery_life.remaining_capacity as f64;

            // get time remaining in seconds
            let time_remaining = battery_life.remaining_time.as_secs() as i32;

            let charging = system.on_ac_power().ok()?;

            let (time_to_empty, time_to_full) = if charging {
                (-1, time_remaining) // time to full  
            } else {
                (time_remaining, -1) // time to empty
            };

            Some((charging, percentage, time_to_empty, time_to_full))
        }

        fn start_monitoring(&self) {
            let obj = self.obj().clone();

            // monitor battery changes every 10 seconds
            glib::timeout_add_local(std::time::Duration::from_secs(10), move || {
                if let Some((charging, percentage, time_to_empty, time_to_full)) =
                    obj.imp().read_battery_state()
                {
                    // only update if values changed to avoid unnecessary signals
                    if (obj.percentage() - percentage).abs() > 0.01 {
                        obj.set_percentage(percentage);
                    }

                    if obj.charging() != charging {
                        obj.set_charging(charging);
                    }

                    if obj.time_to_empty() != time_to_empty {
                        obj.set_time_to_empty(time_to_empty);
                    }

                    if obj.time_to_full() != time_to_full {
                        obj.set_time_to_full(time_to_full);
                    }
                }
                glib::ControlFlow::Continue
            });
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

    pub fn is_low(&self) -> bool {
        let time_remaining = if self.charging() {
            self.time_to_full()
        } else {
            self.time_to_empty()
        };
        (self.percentage() <= 0.2 || (time_remaining > 0 && time_remaining <= 3600))
            && !self.charging()
    }

    pub fn is_critical(&self) -> bool {
        let time_remaining = if self.charging() {
            self.time_to_full()
        } else {
            self.time_to_empty()
        };
        (self.percentage() <= 0.1 || (time_remaining > 0 && time_remaining <= 1800))
            && !self.charging()
    }

    pub fn get_readable_time(&self) -> String {
        if self.charging() && self.percentage() > 0.99 {
            return "Plugged in".to_string();
        }

        let time_remaining = if self.charging() {
            self.time_to_full()
        } else {
            self.time_to_empty()
        };

        if time_remaining < 30 * 60 {
            let minutes = (time_remaining + 59) / 60; // round up
            return format!("{} min left", minutes);
        }

        // calculate actual completion time
        let now = Local::now();
        let completion_time = now + chrono::Duration::seconds(time_remaining as i64);

        // format as "h:mm am/pm" (matches TypeScript DATE_FORMAT)
        let formatted = completion_time.format("%-I:%M %P").to_string();

        if self.charging() {
            format!("Full at {}", formatted)
        } else {
            format!("Until {}", formatted)
        }
    }
}
