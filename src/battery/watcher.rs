use std::{path::Path, time::Duration};

use chrono::Local;
use tokio::io::unix::AsyncFd;

use super::{BATTERY_STATE, BatteryState, ChargingStatus};
use crate::battery::{
    READ_INTERVAL_SECONDS,
    alerts::AlertState,
    discharging::DischargeProfile,
    sysfs::{detect_battery_path, read_battery_sysfs},
    udev::{create_battery_monitor, is_battery_change},
};

pub async fn start_battery_service() {
    // detect battery sysfs path
    let Some(battery_path) = detect_battery_path() else {
        tracing::error!("couldn't detect battery interface");
        return;
    };

    tracing::info!("battery device: {}", battery_path.display());

    // load or create power usage history
    let mut power_history = match DischargeProfile::read_from_disk() {
        Ok(p) => {
            tracing::info!("loaded power history from previous session");
            p
        }
        Err(e) => {
            tracing::info!("creating new power usage log: {}", e);
            DischargeProfile::default()
        }
    };

    // read initial battery properties
    let Some(reading) = read_battery_sysfs(&battery_path) else {
        tracing::error!(
            "couldn't read {}; does it expose voltage_now and power_now or current_now?",
            battery_path.display()
        );
        return;
    };

    // get initial time estimate
    let discharging_time_remaining =
        power_history.predict_time_to_empty(Local::now(), reading.remaining_wh());

    if let Some(percentage) = reading.percentage() {
        *BATTERY_STATE.write() = Some(BatteryState {
            percentage: percentage as f32,
            status: reading.status,
            discharging_time_remaining,
        });
    }

    // set up udev monitor for immediate status change events
    let monitor = match create_battery_monitor() {
        Ok(m) => m,
        Err(e) => {
            tracing::error!("couldn't create udev battery monitor: {}", e);
            return;
        }
    };

    let async_fd = match AsyncFd::new(monitor) {
        Ok(fd) => fd,
        Err(e) => {
            tracing::error!("couldn't wrap udev monitor in AsyncFd: {}", e);
            return;
        }
    };

    let mut alert_state = AlertState::default();

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
                        tracing::error!("error acquiring guard: {e}");
                        continue;
                    },
                };

                tracing::debug!("async_fd ready: {:?}", guard.ready());

                // drain all pending events from the monitor
                for event in guard.get_inner().iter() {
                    tracing::debug!("udev event received: {event:?}");
                    if is_battery_change(&event, battery_path) {
                        tracing::debug!("event is a battery event");
                        update_battery_state(
                            battery_path,
                            power_history,
                            alert_state,
                        ).await;
                    } else {
                        tracing::debug!("event was not a battery event");
                    }
                }

                tracing::debug!("reaching end of socket iterator");

                // clear readiness so we wait for the next edge
                guard.clear_ready();
            }

            _ = poll_interval.tick() => {
                tracing::debug!("battery poll interval elapsed, fetching battery info");
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
        tracing::warn!("couldn't read battery sysfs");
        return;
    };

    if let ChargingStatus::Discharging = reading.status {
        power_history.update(&reading);
    }

    let discharging_time_remaining =
        power_history.predict_time_to_empty(Local::now(), reading.remaining_wh());

    // an unknown level would show as 0% and fire the critical alert
    let Some(percentage) = reading.percentage() else {
        tracing::warn!("battery reported a full capacity of 0; skipping this reading");
        return;
    };
    let percentage = percentage as f32;
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
