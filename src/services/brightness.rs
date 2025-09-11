use std::{fs, path::Path, sync::mpsc};

use anyhow::Result;
use notify::{RecursiveMode, Watcher};
use relm4::Worker;

#[derive(Debug)]
pub enum BrightnessEvent {
    /// The percentage of brightness has changed.
    Percentage(f64),

    /// The service is unavailable.
    Unavailable,
}

#[derive(Clone, Default)]
pub struct BrightnessService;

impl Worker for BrightnessService {
    type Init = ();
    type Input = ();
    type Output = BrightnessEvent;

    fn init(_init: Self::Init, sender: relm4::ComponentSender<Self>) -> Self {
        // read initial backlight properties. if any fail, we will not consider the
        // service available.
        let Ok((interface, max_val, mut current_brightness)) = detect_interface()
            .map_err(|e| log::error!("couldn't detect brightness interface: {}", e))
            .and_then(|interface| {
                read_max_brightness(&interface)
                    .map_err(|e| log::error!("couldn't read max brightness value: {}", e))
                    .and_then(|max_val| {
                        read_current_brightness_percentage(&interface, max_val)
                            .map_err(|e| {
                                log::error!("couldn't read current brightness value: {}", e)
                            })
                            .map(|brightness| (interface, max_val, brightness))
                    })
            })
        else {
            sender
                .output(BrightnessEvent::Unavailable)
                .unwrap_or_else(|_| log::error!("couldn't send unavailability message"));
            return Default::default();
        };

        // send initial update
        sender
            .output(BrightnessEvent::Percentage(current_brightness))
            .unwrap_or_else(|_| {
                log::error!("couldn't send initial state");
            });

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
                                            if new_brightness != current_brightness {
                                                current_brightness = new_brightness;
                                                sender
                                                    .output(BrightnessEvent::Percentage(
                                                        new_brightness,
                                                    ))
                                                    .unwrap_or_else(|_| {
                                                        log::error!(
                                                            "couldn't send brightness update",
                                                        );
                                                    });
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

        BrightnessService
    }

    // inputs are ignored
    fn update(&mut self, _message: Self::Input, _sender: relm4::ComponentSender<Self>) {}
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
