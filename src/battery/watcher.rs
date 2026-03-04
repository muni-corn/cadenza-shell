use std::{path::Path, time::Duration};

use chrono::Local;
use tokio::io::unix::AsyncFd;

use super::{BATTERY_STATE, BatteryState, ChargingStatus};
use crate::battery::{
    charging::{ChargeProfile, ChargingSession, SessionReading, predict_time_to_full_cc_cv},
    history::HistoricalPowerUsage,
    sysfs::{SysfsReading, detect_battery_path, read_battery_sysfs},
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

    // load or create the CC/CV charge profile
    let mut charge_profile = ChargeProfile::load();

    // read initial battery properties. if it fails, we will not consider the
    // service available.
    let Some(reading) = read_battery_sysfs(&battery_path) else {
        return;
    };

    // start a charging session if we are already plugged in on boot
    let mut active_session: Option<ChargingSession> = if reading.status == ChargingStatus::Charging
    {
        Some(ChargingSession::default())
    } else {
        None
    };

    // get initial time estimate
    let time_remaining = compute_time_remaining(
        &reading,
        active_session.as_ref(),
        &charge_profile,
        &mut power_history,
    );

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

    watch_battery(
        &battery_path,
        async_fd,
        &mut power_history,
        &mut charge_profile,
        &mut active_session,
    )
    .await;
}

async fn watch_battery(
    battery_path: &Path,
    async_fd: AsyncFd<udev::MonitorSocket>,
    power_history: &mut HistoricalPowerUsage,
    charge_profile: &mut ChargeProfile,
    active_session: &mut Option<ChargingSession>,
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
                        update_battery_state(
                            battery_path,
                            power_history,
                            charge_profile,
                            active_session,
                        );
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
                    charge_profile,
                    active_session,
                );
            }
        }
    }
}

/// Read the latest battery stats from sysfs and update [`BATTERY_STATE`].
fn update_battery_state(
    battery_path: &Path,
    power_history: &mut HistoricalPowerUsage,
    charge_profile: &mut ChargeProfile,
    active_session: &mut Option<ChargingSession>,
) {
    let Some(reading) = read_battery_sysfs(battery_path) else {
        log::warn!("couldn't read battery sysfs");
        return;
    };

    manage_session(reading.status, &reading, charge_profile, active_session);

    if let ChargingStatus::Discharging = reading.status {
        power_history.update(&reading);
    }

    let time_remaining = compute_time_remaining(
        &reading,
        active_session.as_ref(),
        charge_profile,
        power_history,
    );

    *BATTERY_STATE.write() = Some(BatteryState {
        percentage: reading.percentage().unwrap_or_default() as f32,
        status: reading.status,
        time_remaining,
    });
}

/// Manage the [`ChargingSession`] state machine based on the current status.
///
/// - Creates a new session when charging begins.
/// - Pushes the current reading into an active session.
/// - Ends the session (updating the profile) when charging stops.
fn manage_session(
    status: ChargingStatus,
    reading: &SysfsReading,
    charge_profile: &mut ChargeProfile,
    active_session: &mut Option<ChargingSession>,
) {
    match status {
        ChargingStatus::Charging => {
            // ensure a session is active
            let session = active_session.get_or_insert_with(|| {
                log::info!("charging started, beginning new session");
                ChargingSession::default()
            });

            // push the reading into the session for phase detection
            if let Some(sr) = SessionReading::from_sysfs(reading) {
                session.push(sr, charge_profile);
            }
        }
        _ => {
            // charging stopped — finalise and learn from the session if present
            if let Some(session) = active_session.take() {
                log::info!("charging stopped, finalising session");
                session.end(charge_profile);
            }
        }
    }
}

/// Compute the predicted time remaining using the best available model.
///
/// When charging, prefers the CC/CV model if the session and profile are
/// available. Falls back to the legacy history-based model otherwise.
fn compute_time_remaining(
    reading: &SysfsReading,
    active_session: Option<&ChargingSession>,
    charge_profile: &ChargeProfile,
    power_history: &mut HistoricalPowerUsage,
) -> Duration {
    match reading.status {
        ChargingStatus::Charging => {
            if let Some(session) = active_session {
                let current_ua = reading.current_now.unsigned_abs() as f64;
                let charge_now_uah = reading
                    .capacity_now
                    .as_microampere_hours(reading.voltage_now)
                    as f64;
                let charge_full_uah = reading
                    .capacity_full
                    .as_microampere_hours(reading.voltage_now)
                    as f64;

                predict_time_to_full_cc_cv(
                    session,
                    charge_profile,
                    current_ua,
                    charge_now_uah,
                    charge_full_uah,
                )
            } else {
                log::warn!("there is no active charging session to predict time remaining");
                Duration::MAX
            }
        }
        ChargingStatus::Discharging => {
            power_history.predict_time_to_empty(Local::now(), reading.remaining_wh())
        }
        _ => Duration::MAX,
    }
}
