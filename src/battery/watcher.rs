use std::{path::Path, time::Duration};

use chrono::Local;
use tokio::io::unix::AsyncFd;

use super::{BATTERY_STATE, BatteryState, ChargingStatus};
use crate::battery::{
    READ_INTERVAL_SECONDS,
    alerts::AlertState,
    discharging::DischargeProfile,
    sysfs::{detect_battery_path, read_battery_identity, read_battery_sysfs},
    udev::{create_battery_monitor, is_battery_change},
};

pub async fn start_battery_service() {
    // detect battery sysfs path
    let Some(battery_path) = detect_battery_path() else {
        log::error!("couldn't detect battery interface");
        return;
    };

    // read device identity for per-device profile keying
    let identity = read_battery_identity(&battery_path);
    let device_key = identity.device_key();
    log::info!(
        "battery device: '{}' (key: '{device_key}')",
        identity.sysfs_name,
    );

    // load or create power usage history
    let mut power_history = match DischargeProfile::read_from_disk() {
        Ok(p) => {
            log::info!("loaded power history from previous session");
            p
        }
        Err(e) => {
            log::info!("creating new power usage log: {}", e);
            DischargeProfile::default()
        }
    };

    // read initial battery properties
    let Some(reading) = read_battery_sysfs(&battery_path) else {
        return;
    };

    // get initial time estimate
    let discharging_time_remaining =
        power_history.predict_time_to_empty(Local::now(), reading.remaining_wh());

    *BATTERY_STATE.write() = Some(BatteryState {
        percentage: reading.percentage().unwrap_or_default() as f32,
        status: reading.status,
        discharging_time_remaining,
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

    let mut alert_state = AlertState::new();

    watch_battery(
        &battery_path,
        async_fd,
        &mut power_history,
        &mut alert_state,
    )
    .await;
}

async fn watch_battery(
    battery_path: &Path,
    async_fd: AsyncFd<udev::MonitorSocket>,
    power_history: &mut DischargeProfile,
    alert_state: &mut AlertState,
) -> Option<!> {
    let mut poll_interval =
        tokio::time::interval(Duration::from_secs(READ_INTERVAL_SECONDS.into()));

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
                        update_battery_state(
                            battery_path,
                            power_history,
                            alert_state,
                        ).await;
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
                update_battery_state(
                    battery_path,
                    power_history,
                    alert_state,
                ).await;
            }
        }
    }
}

/// Read the latest battery stats from sysfs, update [`BATTERY_STATE`], and
/// fire any low-battery alerts that have not yet been triggered this session.
async fn update_battery_state(
    battery_path: &Path,
    power_history: &mut DischargeProfile,
    alert_state: &mut AlertState,
) {
    let Some(reading) = read_battery_sysfs(battery_path) else {
        log::warn!("couldn't read battery sysfs");
        return;
    };

    if let ChargingStatus::Discharging = reading.status {
        power_history.update(&reading);
    }

    let discharging_time_remaining =
        power_history.predict_time_to_empty(Local::now(), reading.remaining_wh());

    let percentage = reading.percentage().unwrap_or_default() as f32;
    let status = reading.status;

    *BATTERY_STATE.write() = Some(BatteryState {
        percentage,
        status,
        discharging_time_remaining,
    });

    // check alerts only while discharging; reset flags when we leave that state
    if status == ChargingStatus::Discharging {
        alert_state.check(percentage).await;
    } else {
        alert_state.reset();
    }
}
