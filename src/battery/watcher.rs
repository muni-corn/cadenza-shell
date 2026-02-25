use std::{path::Path, time::Duration};

use chrono::Local;
use tokio::io::{Interest, unix::AsyncFd};

use super::{BATTERY_STATE, BatteryState};
use crate::battery::{
    history::HistoricalPowerUsage,
    sysfs::{detect_battery_path, read_battery_sysfs},
    udev::{create_battery_monitor, is_battery_change},
};

/// Interval between periodic battery stat polls.
const BATTERY_POLL_INTERVAL: Duration = Duration::from_secs(10);

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

    // set up udev monitor for immediate status change events
    let monitor = match create_battery_monitor() {
        Ok(m) => m,
        Err(e) => {
            log::error!("couldn't create udev battery monitor: {}", e);
            return;
        }
    };

    let async_fd = match AsyncFd::new(monitor) {
        Ok(fd) => fd,
        Err(e) => {
            log::error!("couldn't wrap udev monitor in AsyncFd: {}", e);
            return;
        }
    };

    watch_battery(&battery_path, async_fd, &mut power_history).await;
}

async fn watch_battery(
    battery_path: &Path,
    async_fd: AsyncFd<udev::MonitorSocket>,
    power_history: &mut HistoricalPowerUsage,
) -> Option<!> {
    let mut poll_interval = tokio::time::interval(BATTERY_POLL_INTERVAL);

    // skip the first tick, which fires immediately
    poll_interval.tick().await;

    loop {
        tokio::select! {
            guard = async_fd.readable() => {
                let mut guard = match guard {
                    Ok(g) => g,
                    Err(e) => {
                        log::error!("error acquiring guard: {e}");
                        continue;
                    },
                };

                log::debug!("async_fd ready: {:?}", guard.ready());

                // drain all pending events from the monitor
                for event in guard.get_inner().iter() {
                    log::debug!("udev event received: {event:?}");
                    if is_battery_change(&event) {
                        log::debug!("event is a battery event");
                        update_battery_state(battery_path, power_history);
                    } else {
                        log::debug!("event was not a battery event");
                    }
                }

                log::debug!("reaching end of socket iterator");

                // clear readiness so we wait for the next edge
                guard.clear_ready();
            }

            _ = poll_interval.tick() => {
                log::debug!("battery poll interval elapsed, fetching battery info");
                update_battery_state(battery_path, power_history);
            }
        }
    }
}

/// Read the latest battery stats from sysfs and update [`BATTERY_STATE`].
fn update_battery_state(battery_path: &Path, power_history: &mut HistoricalPowerUsage) {
    let Some(reading) = read_battery_sysfs(battery_path) else {
        log::warn!("couldn't read battery sysfs");
        return;
    };

    power_history.update(&reading);

    let time_remaining = power_history.predict_time_remaining(&reading, Local::now());

    *BATTERY_STATE.write() = Some(BatteryState {
        percentage: reading.percentage().unwrap_or_default() as f32,
        status: reading.status,
        time_remaining,
    });
}
