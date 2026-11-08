use std::{fs, path::Path};

use anyhow::{Context, Result};
use futures_lite::StreamExt;
use inotify::{Inotify, WatchMask};
use relm4::SharedState;

use crate::utils::shared_state::set_if_changed;

pub static BRIGHTNESS_STATE: SharedState<Option<f64>> = SharedState::new();

pub async fn start_brightness_watcher() {
    // read initial backlight properties. if any fail, we will not consider the
    // service available.
    let Ok((interface, max_val, current_brightness)) = read_all() else {
        return;
    };

    // send initial update
    *BRIGHTNESS_STATE.write() = Some(current_brightness);

    let brightness_path = format!("/sys/class/backlight/{}/brightness", interface);

    let inotify = match Inotify::init() {
        Ok(inotify) => inotify,
        Err(e) => {
            tracing::error!("failed to init inotify: {}", e);
            return;
        }
    };

    // watch for CLOSE_WRITE events on the brightness file
    let Ok(_wd) = inotify
        .watches()
        .add(&brightness_path, WatchMask::CLOSE_WRITE)
        .map_err(|e| tracing::error!("couldn't set up inotify watch for brightness: {}", e))
    else {
        return;
    };

    // the async stream yields to the runtime between events instead of
    // parking a worker thread on a blocking read
    let mut events = match inotify.into_event_stream([0; 1024]) {
        Ok(events) => events,
        Err(e) => {
            tracing::error!("couldn't stream inotify events for brightness: {e}");
            return;
        }
    };

    while let Some(event) = events.next().await {
        if let Err(e) = event {
            tracing::error!("error while reading inotify events: {e}");
            break;
        }

        // when the brightness file is closed after writing, read the new value
        match read_current_brightness_percentage(&interface, max_val) {
            Ok(percentage) => {
                set_if_changed(&BRIGHTNESS_STATE, Some(percentage));
            }
            Err(e) => tracing::error!("couldn't update brightness info: {e}"),
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

/// Detects, reads, and returns the backlight interface, the max brightness
/// value, and the current brightness value.
fn read_all() -> Result<(String, u32, f64)> {
    let interface = detect_interface().context("couldn't detect brightness interface")?;
    let max_val = read_max_brightness(&interface).context("couldn't read max brightness value")?;
    let brightness = read_current_brightness_percentage(&interface, max_val)
        .context("couldn't read current brightness value")?;

    Ok((interface, max_val, brightness))
}
