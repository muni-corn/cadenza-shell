use std::{fs, path::Path};

use anyhow::Result;
use inotify::{Inotify, WatchMask};
use relm4::{Reducer, Reducible};

pub static BRIGHTNESS_STATE: Reducer<BrightnessState> = Reducer::new();

pub struct BrightnessState(pub Option<f64>);

impl Reducible for BrightnessState {
    type Input = f64;

    fn init() -> Self {
        relm4::spawn(start_brightness_watcher());
        Self(None)
    }

    fn reduce(&mut self, input: Self::Input) -> bool {
        if Some(input) != self.0 {
            self.0 = Some(input);
            true
        } else {
            false
        }
    }
}

pub async fn start_brightness_watcher() {
    // read initial backlight properties. if any fail, we will not consider the
    // service available.
    let Ok((interface, max_val, current_brightness)) = detect_interface()
        .map_err(|e| log::error!("couldn't detect brightness interface: {}", e))
        .and_then(|interface| {
            read_max_brightness(&interface)
                .map_err(|e| log::error!("couldn't read max brightness value: {}", e))
                .and_then(|max_val| {
                    read_current_brightness_percentage(&interface, max_val)
                        .map_err(|e| log::error!("couldn't read current brightness value: {}", e))
                        .map(|brightness| (interface, max_val, brightness))
                })
        })
    else {
        return;
    };

    // send initial update
    BRIGHTNESS_STATE.emit(current_brightness);

    let brightness_path = format!("/sys/class/backlight/{}/brightness", &interface);
    let interface_clone = interface.clone();

    let mut inotify = match Inotify::init() {
        Ok(inotify) => inotify,
        Err(e) => {
            log::error!("failed to init inotify: {}", e);
            return;
        }
    };

    // watch for CLOSE_WRITE events on the brightness file
    let Ok(_wd) = inotify
        .watches()
        .add(&brightness_path, WatchMask::CLOSE_WRITE)
        .map_err(|e| log::error!("couldn't set up inotify watch for brightness: {}", e))
    else {
        return;
    };

    let mut buffer = [0; 1024];
    loop {
        match inotify.read_events_blocking(&mut buffer) {
            Ok(events) => {
                for _ in events {
                    // when the brightness file is closed after writing, read the new value
                    match read_current_brightness_percentage(&interface_clone, max_val) {
                        Ok(percentage) => BRIGHTNESS_STATE.emit(percentage),
                        Err(e) => log::error!("couldn't update brightness info: {}", e),
                    }
                }
            }
            Err(e) => {
                log::error!("error while reading inotify events: {}", e);
                break;
            }
        }
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
