use std::{fs, path::Path, sync::mpsc, time::Duration};

use anyhow::Result;
use notify::{RecursiveMode, Watcher};
use relm4::{Reducer, Reducible};
use systemstat::{Platform, System};

pub static BATTERY_STATE: Reducer<BatteryState> = Reducer::new();

#[derive(Debug, Copy, Clone, PartialEq, Default)]
pub enum BatteryState {
    #[default]
    Unavailable,
    Available {
        percentage: f32,
        charging: bool,
        time_remaining: Duration,
    },
}

impl Reducible for BatteryState {
    type Input = BatteryUpdate;

    fn init() -> Self {
        relm4::spawn(start_battery_watcher());

        Self::Unavailable
    }

    fn reduce(&mut self, input: Self::Input) -> bool {
        let Self::Available {
            percentage,
            charging,
            time_remaining,
        } = self
        else {
            match input {
                BatteryUpdate::PercentLeft(p) => {
                    *self = Self::Available {
                        percentage: p,
                        charging: false,
                        time_remaining: Duration::ZERO,
                    }
                }
                BatteryUpdate::Charging(c) => {
                    *self = Self::Available {
                        percentage: 0.,
                        charging: c,
                        time_remaining: Duration::ZERO,
                    }
                }
                BatteryUpdate::TimeRemaining(duration) => {
                    *self = Self::Available {
                        percentage: 0.,
                        charging: false,
                        time_remaining: duration,
                    }
                }
            };

            return true;
        };

        match input {
            BatteryUpdate::PercentLeft(p) => *percentage = p,
            BatteryUpdate::Charging(c) => *charging = c,
            BatteryUpdate::TimeRemaining(duration) => *time_remaining = duration,
        }

        true
    }
}

pub enum BatteryUpdate {
    PercentLeft(f32),
    Charging(bool),
    TimeRemaining(Duration),
}

async fn start_battery_watcher() {
    // detect battery interface
    let Some(battery_interface) = detect_battery_interface() else {
        log::error!("couldn't detect battery interface");
        return;
    };

    let system = System::new();

    // read initial battery properties. if any fail, battery information will not be
    // available.
    if let Err(e) = update_entire_battery_state(&system) {
        log::error!("couldn't read initial battery state: {}", e);
        return;
    };

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

    loop {
        // waits on file changes, or polls every 30 seconds
        if let Err(mpsc::RecvTimeoutError::Disconnected) = rx.recv_timeout(Duration::from_secs(30))
        {
            log::error!("battery status watcher has died");
            break;
        } else {
            // just poll every 30 seconds without a watcher
            tokio::time::sleep(Duration::from_secs(30)).await
        }

        if let Err(e) = update_entire_battery_state(&system) {
            log::error!("couldn't read battery state: {}", e);
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

/// Feeds the BatteryState Reducer with the percentage remaining, whether the
/// battery is charging, and how much time is remaining.
fn update_entire_battery_state(system: &System) -> Result<()> {
    let battery_info = system.battery_life()?;

    BATTERY_STATE.emit(BatteryUpdate::PercentLeft(battery_info.remaining_capacity));
    BATTERY_STATE.emit(BatteryUpdate::TimeRemaining(battery_info.remaining_time));

    // update charging state last, since it seems fallible
    BATTERY_STATE.emit(BatteryUpdate::Charging(system.on_ac_power()?));

    Ok(())
}
