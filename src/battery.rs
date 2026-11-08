use std::time::Duration;

use relm4::SharedState;

mod alerts;
mod discharging;
mod sysfs;
mod udev;
mod watcher;

use chrono::TimeDelta;
pub use watcher::start_battery_service;

pub static BATTERY_STATE: SharedState<Option<BatteryState>> = SharedState::new();

/// For a moving average over 10 readings.
const STATISTICS_ALPHA: f64 = 1.0 / 10.0;

/// How often to save profiles to disk.
const SAVE_INTERVAL: TimeDelta = TimeDelta::seconds(30);

/// How often to read the battery state.
const READ_INTERVAL_SECONDS: u32 = 10;

#[derive(Debug, Copy, Clone, PartialEq, Default)]
pub struct BatteryState {
    pub percentage: f32,
    pub status: ChargingStatus,
    pub discharging_time_remaining: Duration,
}

#[derive(Clone, Copy, Debug)]
pub enum BatteryCapacity {
    /// µAh
    MicroAmpereHours(u64),

    /// µWh
    MicroWattHours(u64),
}

impl BatteryCapacity {
    /// Returns `self / rhs`, or `None` if the units differ or `rhs` is zero.
    pub fn ratio(self, rhs: Self) -> Option<f64> {
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
#[derive(Default, Debug, Copy, Clone, PartialEq, Eq)]
pub enum ChargingStatus {
    Charging,
    Discharging,
    Full,
    NotCharging,

    #[default]
    Unknown,
}

impl ChargingStatus {
    pub fn is_charging(&self) -> bool {
        matches!(self, Self::Charging | Self::Full)
    }
}
