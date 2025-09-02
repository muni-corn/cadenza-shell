use std::fs;

use anyhow::Result;
use gtk4::{glib, subclass::prelude::*};

mod imp {
    use std::{
        cell::{Cell, RefCell},
        fs,
        path::Path,
        sync::mpsc,
    };

    use anyhow::Result;
    use gtk4::{glib, prelude::*, subclass::prelude::*};
    use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};

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
        _watcher: RefCell<Option<RecommendedWatcher>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for BrightnessService {
        type ParentType = glib::Object;
        type Type = super::BrightnessService;

        const NAME: &'static str = "MuseShellBrightnessService";
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
            // read min/max brightness from sysfs directly
            let interface = self.interface.borrow();
            let max_path = format!("/sys/class/backlight/{}/max_brightness", interface);
            let max_content = fs::read_to_string(max_path)?;
            let max_val: u32 = max_content.trim().parse()?;
            Ok((0, max_val))
        }

        fn start_monitoring(&self) {
            let obj = self.obj().clone();
            let interface = self.interface.borrow().clone();
            let brightness_path = format!("/sys/class/backlight/{}/brightness", interface);

            // create channel for file system events
            let (tx, rx) = mpsc::channel();

            // create file watcher with notify v8 api
            let watcher_result = RecommendedWatcher::new(
                move |res: notify::Result<notify::Event>| {
                    if let Ok(event) = res {
                        // check if it's a modify event
                        if matches!(event.kind, notify::EventKind::Modify(_)) {
                            let _ = tx.send(());
                        }
                    }
                },
                Config::default(),
            );

            match watcher_result {
                Ok(mut watcher) => {
                    // watch the brightness file
                    if let Err(e) =
                        watcher.watch(&Path::new(&brightness_path), RecursiveMode::NonRecursive)
                    {
                        log::warn!("failed to watch brightness file: {}", e);
                        return;
                    }

                    // store watcher to keep it alive
                    self._watcher.replace(Some(watcher));

                    // use timeout instead of async future to avoid blocking recv()
                    let obj_clone = obj.clone();
                    glib::timeout_add_local(std::time::Duration::from_millis(50), move || {
                        // non-blocking check for file system events
                        match rx.try_recv() {
                            Ok(_) => {
                                // file changed, update brightness
                                if let Ok(brightness) = obj_clone.imp().read_current_brightness() {
                                    obj_clone.set_brightness(brightness);
                                }
                            }
                            Err(mpsc::TryRecvError::Empty) => {
                                // no events, continue
                            }
                            Err(mpsc::TryRecvError::Disconnected) => {
                                // watcher disconnected, stop timeout
                                return glib::ControlFlow::Break;
                            }
                        }
                        glib::ControlFlow::Continue
                    });

                    // schedule initial value read on next tick to avoid blocking
                    let obj_initial = obj.clone();
                    glib::idle_add_local_once(move || {
                        if let Ok(brightness) = obj_initial.imp().read_current_brightness() {
                            obj_initial.set_brightness(brightness);
                        }
                    });
                }
                Err(e) => {
                    log::warn!(
                        "failed to create file watcher, falling back to polling: {}",
                        e
                    );
                    // fallback to polling
                    glib::timeout_add_local(std::time::Duration::from_millis(500), move || {
                        if let Ok(brightness) = obj.imp().read_current_brightness() {
                            obj.set_brightness(brightness);
                        }
                        glib::ControlFlow::Continue
                    });
                }
            }
        }

        fn read_current_brightness(&self) -> Result<f64> {
            // read brightness from sysfs directly
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

impl Default for BrightnessService {
    fn default() -> Self {
        Self::new()
    }
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

        // write directly to sysfs (requires appropriate permissions)
        let interface = imp.interface();
        let path = format!("/sys/class/backlight/{}/brightness", interface);
        fs::write(path, clamped.to_string()).map_err(|e| e.into())
    }
}
