use std::{path::Path, sync::mpsc, time::Duration};

use chrono::Local;
use notify::{RecursiveMode, Watcher};

use super::{BATTERY_STATE, BatteryState};
use crate::battery::{
    history::HistoricalPowerUsage,
    sysfs::{detect_battery_path, read_battery_sysfs},
};

/// Maximum time between battery information and status fetches.
const MAX_BATTERY_POLL_TIME: Duration = Duration::from_secs(30);

pub async fn start_battery_service() {
    // detect battery sysfs path
    let Some(battery_path) = detect_battery_path() else {
        log::error!("couldn't detect battery interface");
        return;
    };

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

    // read initial battery properties. if it fails, we will not consider the
    // service available.
    let Some(reading) = read_battery_sysfs(&battery_path) else {
        return;
    };

    // get initial time estimate from historical records
    let time_remaining = power_history.predict_time_remaining(&reading, Local::now());

    *BATTERY_STATE.write() = Some(BatteryState {
        percentage: reading.percentage().unwrap_or_default() as f32,
        status: reading.status,
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
    let status_path = battery_path.join("status");

    if let Err(e) = watcher.watch(Path::new(&status_path), RecursiveMode::NonRecursive) {
        log::error!(
            "couldn't set up watcher for {}: {}",
            status_path.to_string_lossy(),
            e
        );
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

        let Some(reading) = read_battery_sysfs(&battery_path) else {
            return;
        };

        // update historical readings with new reading
        power_history.update(&reading);

        // get new time_remaining estimate
        let time_remaining = power_history.predict_time_remaining(&reading, Local::now());

        *BATTERY_STATE.write() = Some(BatteryState {
            percentage: reading.percentage().unwrap_or_default() as f32,
            status: reading.status,
            time_remaining,
        });
    }
}
