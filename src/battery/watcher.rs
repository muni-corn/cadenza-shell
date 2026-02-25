use std::{fs, path::Path, sync::mpsc, time::Duration};

use anyhow::{Context, Result};
use chrono::Local;
use notify::{RecursiveMode, Watcher};
use systemstat::{Platform, System};

use super::{BATTERY_STATE, BatteryState};
use crate::battery::{history::HistoricalPowerUsage, sysfs::read_battery_sysfs};

/// Maximum time between battery information and status fetches.
const MAX_BATTERY_POLL_TIME: Duration = Duration::from_secs(30);

pub async fn start_battery_watcher() {
    // detect battery interface
    let Some(battery_interface) = detect_battery_interface() else {
        log::error!("couldn't detect battery interface");
        return;
    };

    let system = System::new();

    // load or create power usage history
    let mut power_history = match HistoricalPowerUsage::read_from_disk() {
        Ok(p) => {
            log::info!("loaded power history from previous session");
            p
        }
        Err(e) => {
            log::info!("creating new power usage log: {}", e);
            HistoricalPowerUsage::default()
        }
    };

    // read initial battery properties. if any fail, we will not consider the
    // service available.
    let Ok((percentage, charging, time_remaining)) =
        read_battery_state(&system).context("couldn't read initial battery state")
    else {
        return;
    };

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

    // watch status file for charging state changes
    let status_path = format!("/sys/class/power_supply/{}/status", battery_interface);

    if let Err(e) = watcher.watch(Path::new(&status_path), RecursiveMode::NonRecursive) {
        log::error!("couldn't set up watcher for {}: {}", status_path, e);
        return;
    }

    let mut has_watcher = true;

    loop {
        if has_watcher {
            // additional reads of sysfs may have occurred during last iteration, so drain
            // events before waiting again
            while rx.try_recv().is_ok() {}

            // now wait for a file change event or poll timeout
            match rx.recv_timeout(MAX_BATTERY_POLL_TIME) {
                Ok(Err(e)) => {
                    // i'm not sure what this case is supposed to handle
                    log::error!("{e}");

                    // so we won't react to this errant event; continue instead
                    continue;
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    // normal poll interval elapsed, proceed with update
                    log::debug!(
                        "no battery event received for {MAX_BATTERY_POLL_TIME:?}, fetching battery info now"
                    );
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    // no longer sending or receiving events
                    log::error!("battery status watcher has died");
                    has_watcher = false;
                }
                _ => (),
            }
        } else {
            // just poll without a watcher
            tokio::time::sleep(MAX_BATTERY_POLL_TIME).await;
        }

        match read_battery_state(&system) {
            Ok((percentage, charging, dumb_time_remaining)) => {
                // update predictor if sysfs data available
                if let Some(reading) = read_battery_sysfs() {
                    power_history.update(&reading);

                    let time_remaining =
                        power_history.predict_time_remaining(&reading, Local::now());
        // update historical readings with new reading
        power_history.update(&reading);

                    *BATTERY_STATE.write() = Some(BatteryState {
                        percentage,
                        charging,
                        time_remaining,
                    });
                } else {
                    // sysfs unavailable, fall back to kernel estimates
                    *BATTERY_STATE.write() = Some(BatteryState {
                        percentage,
                        charging,
                        time_remaining: dumb_time_remaining,
                    });
                }
            }
            Err(e) => {
                log::error!("couldn't read battery state: {}", e);
            }
        }
    }
}

/// Detect the battery interface by scanning /sys/class/power_supply/ for
/// devices with type "Battery".
fn detect_battery_interface() -> Option<String> {
    let power_supply_path = Path::new("/sys/class/power_supply");

    // read all entries in the power_supply directory
    fs::read_dir(power_supply_path).ok()?.find_map(|entry| {
        let entry = entry.ok()?;
        let path = entry.path();

        // check if this is a directory
        if path.is_dir() {
            // check if there's a type file
            let type_path = path.join("type");
            if type_path.exists() {
                // read the type file
                let type_content = fs::read_to_string(&type_path).ok()?;
                if type_content.trim().eq_ignore_ascii_case("battery") {
                    // found a battery device, return its name
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
