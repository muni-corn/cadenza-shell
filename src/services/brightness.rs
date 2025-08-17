use anyhow::Result;
use gtk4::glib;
use gtk4::subclass::prelude::*;
use std::fs;

mod imp {
    use anyhow::Result;
    use gtk4::glib;
    use gtk4::prelude::*;
    use gtk4::subclass::prelude::*;
    use std::cell::{Cell, RefCell};
    use std::fs;
    use std::path::Path;

    #[derive(glib::Properties, Default)]
    #[properties(wrapper_type = super::BrightnessService)]
    pub struct BrightnessService {
        #[property(get, set)]
        available: Cell<bool>,

        #[property(get, set, minimum = 0.0, maximum = 1.0)]
        brightness: Cell<f64>,

        interface: RefCell<String>,
        min: Cell<u32>,
        max: Cell<u32>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for BrightnessService {
        const NAME: &'static str = "MuseShellBrightnessService";
        type Type = super::BrightnessService;
        type ParentType = glib::Object;
    }

    #[glib::derived_properties]
    impl ObjectImpl for BrightnessService {
        fn constructed(&self) {
            self.parent_constructed();

            // Initialize brightness monitoring
            if let Ok(interface) = self.detect_interface() {
                self.interface.replace(interface.clone());

                if let Ok((min, max)) = self.read_brightness_range() {
                    self.min.set(min);
                    self.max.set(max);
                    self.available.set(true);

                    // Start monitoring
                    self.start_monitoring();
                }
            }
        }
    }

    impl BrightnessService {
        pub fn min(&self) -> u32 {
            self.min.get()
        }

        pub fn max(&self) -> u32 {
            self.max.get()
        }

        pub fn interface(&self) -> String {
            self.interface.borrow().clone()
        }

        fn detect_interface(&self) -> Result<String> {
            let backlight_path = Path::new("/sys/class/backlight");
            let mut entries = fs::read_dir(backlight_path)?;

            if let Some(entry) = entries.next() {
                let entry = entry?;
                return Ok(entry.file_name().to_string_lossy().to_string());
            }

            anyhow::bail!("No backlight interface found")
        }

        fn read_brightness_range(&self) -> Result<(u32, u32)> {
            // Read min/max brightness using brillo or direct sysfs access
            let min_result = std::process::Command::new("brillo").args(&["-rc"]).output();
            let max_result = std::process::Command::new("brillo").args(&["-rm"]).output();

            match (min_result, max_result) {
                (Ok(min), Ok(max)) => {
                    let min_val: u32 = String::from_utf8_lossy(&min.stdout).trim().parse()?;
                    let max_val: u32 = String::from_utf8_lossy(&max.stdout).trim().parse()?;
                    Ok((min_val, max_val))
                }
                _ => {
                    // Fallback to reading from sysfs directly
                    let interface = self.interface.borrow();
                    let max_path = format!("/sys/class/backlight/{}/max_brightness", interface);
                    let max_content = fs::read_to_string(max_path)?;
                    let max_val: u32 = max_content.trim().parse()?;
                    Ok((0, max_val))
                }
            }
        }

        fn start_monitoring(&self) {
            let obj = self.obj().clone();

            glib::timeout_add_local(std::time::Duration::from_millis(500), move || {
                if let Ok(brightness) = obj.imp().read_current_brightness() {
                    obj.set_brightness(brightness);
                }
                glib::ControlFlow::Continue
            });
        }

        fn read_current_brightness(&self) -> Result<f64> {
            // Try brillo first
            if let Ok(output) = std::process::Command::new("brillo").args(&["-rG"]).output() {
                if let Ok(raw_str) = String::from_utf8(output.stdout) {
                    if let Ok(raw) = raw_str.trim().parse::<u32>() {
                        let min = self.min.get();
                        let max = self.max.get();
                        return Ok((raw - min) as f64 / (max - min) as f64);
                    }
                }
            }

            // Fallback to reading from sysfs
            let interface = self.interface.borrow();
            let path = format!("/sys/class/backlight/{}/brightness", interface);
            let content = fs::read_to_string(path)?;
            let raw: u32 = content.trim().parse()?;

            let min = self.min.get();
            let max = self.max.get();
            Ok((raw - min) as f64 / (max - min) as f64)
        }
    }
}

glib::wrapper! {
    pub struct BrightnessService(ObjectSubclass<imp::BrightnessService>);
}

impl BrightnessService {
    pub fn new() -> Self {
        glib::Object::builder().build()
    }

    pub fn set_brightness_value(&self, percent: f64) -> Result<()> {
        let imp = self.imp();
        let min = imp.min();
        let max = imp.max();
        let raw_value = min + ((max - min) as f64 * percent) as u32;
        let clamped = raw_value.clamp(min, max);

        // Use brillo for privileged write
        let result = std::process::Command::new("brillo")
            .args(&["-Sr", &clamped.to_string()])
            .output();

        match result {
            Ok(_) => {
                // The property will be updated by the monitoring loop
                Ok(())
            }
            Err(e) => {
                log::warn!("Failed to set brightness using brillo: {}", e);
                // Try direct sysfs write (requires appropriate permissions)
                let interface = imp.interface();
                let path = format!("/sys/class/backlight/{}/brightness", interface);
                fs::write(path, clamped.to_string()).map_err(|e| e.into())
            }
        }
    }
}
