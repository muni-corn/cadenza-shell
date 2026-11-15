use std::{
    path::Path,
    time::{Duration, Instant},
};

use tokio::sync::mpsc;

use super::{BATTERY_STATE, BatteryState, ChargingStatus};
use crate::{
    battery::{
        READ_INTERVAL_SECONDS,
        alerts::AlertState,
        estimates::PowerModel,
        sysfs::{detect_battery_path, read_battery_sysfs},
        upower::run_upower_trigger,
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

    let Some(battery_name) = battery_path.file_name() else {
        tracing::error!("battery path {} has no name", battery_path.display());
        return;
    };

    // upower reports status changes right away; without it the poll interval
    // still keeps the tile current, just less promptly
    let (changed_tx, changed_rx) = mpsc::channel(1);

    let mut alert_state = AlertState::default();

    tokio::join!(
        run_upower_trigger(battery_name.to_owned(), changed_tx),
        watch_battery(
            &battery_path,
            changed_rx,
            &mut power_model,
            &mut alert_state
        ),
    );
}

/// Updates the battery state on every upower change for the battery and on
/// every poll tick, forever.
async fn watch_battery(
    battery_path: &Path,
    mut changed: mpsc::Receiver<()>,
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
            Some(()) = changed.recv() => {
                update_battery_state(battery_path, power_model, alert_state).await;
            }

            _ = poll_interval.tick() => {
                update_battery_state(battery_path, power_model, alert_state).await;
            }

            Ok(()) = wake.recv() => {
                update_battery_state(battery_path, power_model, alert_state).await;
            }
        }
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

    // check alerts only while discharging, and re-arm them only once the
    // battery actually charges; brief unknown or not-charging readings
    // would otherwise make them fire again
    if status == ChargingStatus::Discharging {
        alert_state.check(percentage).await;
    } else if status.is_charging() {
        alert_state.reset();
    }
}
