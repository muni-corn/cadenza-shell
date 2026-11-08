use std::{
    path::Path,
    time::{Duration, Instant},
};

use tokio::io::unix::{AsyncFd, AsyncFdReadyGuard};

use super::{BATTERY_STATE, BatteryState, ChargingStatus};
use crate::{
    battery::{
        READ_INTERVAL_SECONDS,
        alerts::AlertState,
        estimates::PowerModel,
        sysfs::{detect_battery_path, read_battery_sysfs},
        udev::{create_battery_monitor, is_battery_change},
    },
    sleep_monitor,
};

pub async fn start_battery_service() {
    // detect battery sysfs path
    let Some(battery_path) = detect_battery_path() else {
        tracing::error!("couldn't detect battery interface");
        return;
    };

    tracing::info!("battery device: {}", battery_path.display());

    let mut power_model = PowerModel::default();

    // read initial battery properties
    let Some(reading) = read_battery_sysfs(&battery_path) else {
        tracing::error!(
            "couldn't read {}; does it expose voltage_now and power_now or current_now?",
            battery_path.display()
        );
        return;
    };

    if reading.status == ChargingStatus::Discharging {
        power_model.update(reading.power_watts, Instant::now());
    }
    let discharging_time_remaining =
        power_model.time_to_empty(reading.remaining_wh(), reading.capacity_wh());

    if let Some(percentage) = reading.percentage() {
        *BATTERY_STATE.write() = Some(BatteryState {
            percentage: percentage as f32,
            status: reading.status,
            discharging_time_remaining,
        });
    }

    // udev reports status changes right away; without it the poll interval
    // still keeps the tile current, just less promptly
    let udev = match create_battery_monitor().and_then(|monitor| Ok(AsyncFd::new(monitor)?)) {
        Ok(fd) => Some(fd),
        Err(e) => {
            tracing::warn!("couldn't monitor udev for battery changes, polling instead: {e}");
            None
        }
    };

    let mut alert_state = AlertState::default();

    watch_battery(&battery_path, udev, &mut power_model, &mut alert_state).await;
}

/// Updates the battery state on every udev change event for the battery and
/// on every poll tick, forever.
async fn watch_battery(
    battery_path: &Path,
    mut udev: Option<AsyncFd<udev::MonitorSocket>>,
    power_model: &mut PowerModel,
    alert_state: &mut AlertState,
) {
    let mut poll_interval =
        tokio::time::interval(Duration::from_secs(READ_INTERVAL_SECONDS.into()));

    // skip the first tick, which fires immediately
    poll_interval.tick().await;

    // the poll interval pauses during suspend, so refresh as soon as we wake
    let mut wake = sleep_monitor::subscribe_wake();

    loop {
        tokio::select! {
            ready = udev_readable(udev.as_ref()) => match ready {
                Ok(mut guard) => {
                    // drain every pending event, but update once per batch
                    let battery_changed = guard
                        .get_inner()
                        .iter()
                        .filter(|event| is_battery_change(event, battery_path))
                        .count()
                        > 0;
                    guard.clear_ready();

                    if battery_changed {
                        update_battery_state(battery_path, power_model, alert_state).await;
                    }
                }
                Err(e) => {
                    tracing::error!("udev battery monitor failed, polling instead: {e}");
                    udev = None;
                }
            },

            _ = poll_interval.tick() => {
                update_battery_state(battery_path, power_model, alert_state).await;
            }

            Ok(()) = wake.recv() => {
                update_battery_state(battery_path, power_model, alert_state).await;
            }
        }
    }
}

/// Waits for the udev monitor to become readable, or forever without one.
async fn udev_readable(
    udev: Option<&AsyncFd<udev::MonitorSocket>>,
) -> std::io::Result<AsyncFdReadyGuard<'_, udev::MonitorSocket>> {
    match udev {
        Some(fd) => fd.readable().await,
        None => std::future::pending().await,
    }
}

/// Read the latest battery stats from sysfs, update [`BATTERY_STATE`], and
/// fire any low-battery alerts that have not yet been triggered this session.
async fn update_battery_state(
    battery_path: &Path,
    power_model: &mut PowerModel,
    alert_state: &mut AlertState,
) {
    let Some(reading) = read_battery_sysfs(battery_path) else {
        tracing::warn!("couldn't read battery sysfs");
        return;
    };

    if reading.status == ChargingStatus::Discharging {
        power_model.update(reading.power_watts, Instant::now());
    }

    // while charging this is how long the battery would last if unplugged
    let discharging_time_remaining =
        power_model.time_to_empty(reading.remaining_wh(), reading.capacity_wh());

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
