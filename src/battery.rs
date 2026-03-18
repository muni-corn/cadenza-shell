use std::time::Duration;

use relm4::SharedState;

mod charging;
mod cv_fit;
mod discharging;
mod sysfs;
mod udev;
mod watcher;

use chrono::TimeDelta;
use serde::{Deserialize, Serialize};
pub use watcher::start_battery_service;

pub static BATTERY_STATE: SharedState<Option<BatteryState>> = SharedState::new();

/// For a moving average over 10 readings.
const STATISTICS_ALPHA: f64 = 1. / 10.;

/// How often to save profiles to disk.
const SAVE_INTERVAL: TimeDelta = TimeDelta::seconds(30);

/// How often to read the battery state.
const READ_INTERVAL_SECONDS: u32 = 10;

#[derive(Debug, Copy, Clone, PartialEq, Default)]
pub struct BatteryState {
    pub percentage: f32,
    pub status: ChargingStatus,
    pub time_remaining: Duration,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub enum BatteryCapacity {
    /// µAh
    MicroAmpereHours(u64),

    /// µWh
    MicroWattHours(u64),
}

impl BatteryCapacity {
    pub fn as_microampere_hours(&self, voltage: u64) -> u64 {
        match *self {
            BatteryCapacity::MicroAmpereHours(uah) => uah,
            BatteryCapacity::MicroWattHours(uwh) if voltage > 0 => uwh * 1_000_000 / voltage,
            _ => u64::MAX,
        }
    }

    pub fn div(self, rhs: Self) -> Option<f64> {
        match (self, rhs) {
            (Self::MicroAmpereHours(l), Self::MicroAmpereHours(r))
            | (Self::MicroWattHours(l), Self::MicroWattHours(r))
                if r > 0 =>
            {
                Some(l as f64 / r as f64)
            }
            _ => None,
        }
    }
}

/// Charging status of the battery.
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub enum ChargingStatus {
    Charging,
    Discharging,
    Full,
    NotCharging,

    #[default]
    Unknown,
}
