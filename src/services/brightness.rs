use std::{
    fs,
    path::Path,
    sync::{Arc, mpsc},
};

use anyhow::Result;
use notify::{RecursiveMode, Watcher};
use tokio::sync::RwLock;

use crate::services::{AsyncProp, Callback, Callbacks, Service};

#[derive(Debug)]
pub enum BrightnessEvent {
    /// The percentage of brightness has changed.
    Percentage(f64),

    /// The service is unavailable.
    Unavailable,
}

#[derive(Clone, Default)]
pub struct BrightnessService {
    available: bool,
    brightness: AsyncProp<f64>, // 0.0 to 1.0
    callbacks: Callbacks<<Self as Service>::Event>,
}

impl Service for BrightnessService {
    type Event = BrightnessEvent;

    fn launch() -> Self {
        // read initial backlight properties. if any fail, we will not consider the
        // service available.
        let interface = match detect_interface() {
            Ok(interface) => interface,
            Err(e) => {
                log::error!("couldn't detect brightness interface: {}", e);
                return Default::default();
            }
        };
        let max_val = match read_max_brightness(&interface) {
            Ok(max_val) => max_val,
            Err(e) => {
                log::error!("couldn't read max brightness value: {}", e);
                return Default::default();
            }
        };
        let brightness = match read_current_brightness_percentage(&interface, max_val) {
            Ok(brightness) => Arc::new(RwLock::new(brightness)),
            Err(e) => {
                log::error!("couldn't read current brightness value: {}", e);
                return Default::default();
            }
        };
        let callbacks = Arc::new(RwLock::new(Vec::new()));

        // yippee!! we've successfully setup the service!
        let service = Self {
            available: true,
            brightness: Arc::clone(&brightness),
            callbacks: Arc::clone(&callbacks),
        };

        tokio::spawn(async move {
            log::debug!("creating channel for watcher");
            let (tx, rx) = mpsc::channel();
            let brightness_path = format!("/sys/class/backlight/{}/brightness", &interface);

            // watch the brightness file
            match notify::recommended_watcher(tx) {
                Ok(mut watcher) => {
                    match watcher.watch(Path::new(&brightness_path), RecursiveMode::NonRecursive) {
                        Ok(_) => {
                            // main loop for file changes
                            loop {
                                if let Err(e) = rx.recv() {
                                    log::error!("brightness file watcher died: {}", e);
                                    break;
                                } else {
                                    match read_current_brightness_percentage(&interface, max_val) {
                                        Ok(new_brightness) => {
                                            let changed =
                                                { new_brightness != *brightness.read().await };
                                            if changed {
                                                *brightness.write().await = new_brightness;
                                                for callback in &mut *callbacks.write().await {
                                                    callback(BrightnessEvent::Percentage(
                                                        new_brightness,
                                                    ));
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            log::error!("couldn't read current brightness: {}", e);
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            log::error!("couldn't set up watcher for brightness: {}", e);
                        }
                    }
                }
                Err(e) => log::error!("couldn't create watcher: {}", e),
            }
        });

        service
    }

    fn with(self, mut callback: impl Callback<Self::Event> + 'static) -> Self {
        log::debug!("adding callback while available is {}", self.available);
        let callbacks = Arc::clone(&self.callbacks);
        let brightness = Arc::clone(&self.brightness);
        let available = self.available;

        tokio::spawn(async move {
            if available {
                log::debug!("calling new callback with current brightness");
                callback(BrightnessEvent::Percentage(*brightness.read().await));
            } else {
                log::debug!("calling new callback with unavailability");
                callback(BrightnessEvent::Unavailable);
            }

            callbacks.write().await.push(Box::new(callback));
            println!("callback added to callbacks list");
        });

        self
    }
}

fn detect_interface() -> Result<String> {
    let backlight_path = Path::new("/sys/class/backlight");
    let mut entries = fs::read_dir(backlight_path)?;

    if let Some(entry) = entries.next() {
        return Ok(entry?.file_name().to_string_lossy().to_string());
    }

    anyhow::bail!("no backlight interface found")
}

fn read_max_brightness(interface: &str) -> Result<u32> {
    let max_path = format!("/sys/class/backlight/{}/max_brightness", interface);
    let max_content = fs::read_to_string(max_path)?;
    let max_val: u32 = max_content.trim().parse()?;
    Ok(max_val)
}

fn read_current_brightness_percentage(interface: &str, max_val: u32) -> Result<f64> {
    let path = format!("/sys/class/backlight/{}/brightness", interface);
    let content = fs::read_to_string(path)?;
    let raw: u32 = content.trim().parse()?;
    Ok(raw as f64 / max_val as f64)
}
