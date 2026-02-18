use std::{fs, path::Path, sync::mpsc, time::Duration};

use anyhow::Result;
use notify::{RecursiveMode, Watcher};
use systemstat::{Platform, System};

use super::{BATTERY_STATE, BatteryPredictor, BatteryState, load_predictor, save_predictor};
use crate::battery::sysfs::read_battery_sysfs;

/// Maximum time between battery information and status fetches.
const MAX_BATTERY_POLL_TIME: Duration = Duration::from_secs(30);

pub async fn start_battery_watcher() {
    // detect battery interface
    let Some(battery_interface) = detect_battery_interface() else {
        log::error!("couldn't detect battery interface");
        return;
    };

    let system = System::new();

    // load or create predictor
    let mut predictor = match load_predictor() {
        Ok(p) => {
            log::info!("loaded battery predictor from previous session");
            p
        }
        Err(e) => {
            log::info!("creating new battery predictor: {}", e);
            BatteryPredictor::new()
        }
    };

    // read initial battery properties. if any fail, we will not consider the
    // service available.
    let Ok((percentage, charging, time_remaining)) = read_battery_state(&system)
        .map_err(|e| log::error!("couldn't read initial battery state: {}", e))
    else {
        return;
    };

    // send initial update with prediction
    let (smart_time_remaining, confidence) = read_battery_sysfs()
        .and_then(|reading| predictor.predict_time_remaining(&reading))
        .unwrap_or((time_remaining, 0.0));

    *BATTERY_STATE.write() = Some(BatteryState {
        percentage,
        charging,
        time_remaining,
        smart_time_remaining,
        confidence,
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
    let mut last_save: Option<Instant> = None;

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
            Ok((percentage, charging, time_remaining)) => {
                // update predictor if sysfs data available
                if let Some(reading) = read_battery_sysfs() {
                    predictor.update(&reading);

                    // get smart prediction
                    let (smart_time_remaining, confidence) = predictor
                        .predict_time_remaining(&reading)
                        .unwrap_or((time_remaining, 0.0));

                    *BATTERY_STATE.write() = Some(BatteryState {
                        percentage,
                        charging,
                        time_remaining,
                        smart_time_remaining,
                        confidence,
                    });
                } else {
                    // sysfs unavailable, fall back to kernel estimates
                    *BATTERY_STATE.write() = Some(BatteryState {
                        percentage,
                        charging,
                        time_remaining,
                        smart_time_remaining: time_remaining,
                        confidence: 0.0,
                    });
                }

                // save predictor state every 10 updates (~5 minutes)
                update_count += 1;
                if update_count.is_multiple_of(10)
                    && let Err(e) = save_predictor(&predictor)
                {
                    log::warn!("couldn't save battery predictor: {}", e);
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
                if type_content.trim() == "Battery" {
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
