use std::{fs, path::Path, sync::mpsc, time::Duration};

use anyhow::Result;
use notify::{RecursiveMode, Watcher};
use relm4::SharedState;
use systemstat::{Platform, System};

pub static BATTERY_STATE: SharedState<Option<BatteryState>> = SharedState::new();

#[derive(Debug, Copy, Clone, PartialEq, Default)]
pub struct BatteryState {
    pub percentage: f32,
    pub charging: bool,
    pub time_remaining: Duration,
}

pub async fn start_battery_watcher() {
    // detect battery interface
    let Some(battery_interface) = detect_battery_interface() else {
        log::error!("couldn't detect battery interface");
        return;
    };

    let system = System::new();

    // read initial battery properties. if any fail, we will not consider the
    // service available.
    let Ok((percentage, charging, time_remaining)) = read_battery_state(&system)
        .map_err(|e| log::error!("couldn't read initial battery state: {}", e))
    else {
        return;
    };

    // send initial update
    *BATTERY_STATE.write() = Some(BatteryState {
        percentage,
        charging,
        time_remaining,
    });

    let (tx, rx) = mpsc::channel();

    // watch only the status file for instant updates
    let mut watcher = match notify::recommended_watcher(tx) {
        Ok(watcher) => watcher,
        Err(e) => {
            log::error!("couldn't create battery watcher: {}", e);
            return;
        }
    };

    // Watch status file for charging state changes
    let status_path = format!("/sys/class/power_supply/{}/status", battery_interface);

    if let Err(e) = watcher.watch(Path::new(&status_path), RecursiveMode::NonRecursive) {
        log::error!("couldn't set up watcher for {}: {}", status_path, e);
        return;
    }

    let mut has_watcher = true;
    loop {
        // waits on file changes, or polls every 30 seconds
        if has_watcher
            && let Err(mpsc::RecvTimeoutError::Disconnected) =
                rx.recv_timeout(Duration::from_secs(30))
        {
            log::error!("battery status watcher has died");
            has_watcher = false;
        } else {
            // just poll every 30 seconds without a watcher
            tokio::time::sleep(Duration::from_secs(30)).await
        }

        match read_battery_state(&system) {
            Ok((percentage, charging, time_remaining)) => {
                *BATTERY_STATE.write() = Some(BatteryState {
                    percentage,
                    charging,
                    time_remaining,
                });
            }
            Err(e) => {
                log::error!("couldn't read battery state: {}", e);
            }
        }
    }
}

/// Detect the battery interface by scanning /sys/class/power_supply/ for
/// devices with type "Battery"
fn detect_battery_interface() -> Option<String> {
    let power_supply_path = Path::new("/sys/class/power_supply");

    // Read all entries in the power_supply directory
    fs::read_dir(power_supply_path).ok()?.find_map(|entry| {
        let entry = entry.ok()?;
        let path = entry.path();

        // Check if this is a directory
        if path.is_dir() {
            // Check if there's a type file
            let type_path = path.join("type");
            if type_path.exists() {
                // Read the type file
                let type_content = fs::read_to_string(&type_path).ok()?;
                if type_content.trim() == "Battery" {
                    // Found a battery device, return its name
                    Some(entry.file_name().to_string_lossy().to_string())
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

/// Returns the percentage remaining, whether the battery is charging, and how
/// much time is remaining.
fn read_battery_state(system: &System) -> Result<(f32, bool, Duration)> {
    let battery_info = system.battery_life()?;

    let percentage = battery_info.remaining_capacity;
    let charging = system.on_ac_power()?;
    let time_remaining = battery_info.remaining_time;

    Ok((percentage, charging, time_remaining))
}
