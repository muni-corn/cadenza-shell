use std::time::Duration;

use relm4::SharedState;

mod alerts;
mod estimates;
mod sysfs;
mod udev;
mod watcher;

pub use watcher::start_battery_service;

pub static BATTERY_STATE: SharedState<Option<BatteryState>> = SharedState::new();

/// Charge level at which the battery is considered low.
pub const BATTERY_THRESHOLD_LOW: f32 = 0.2;

/// Charge level at which the battery is considered critically low.
pub const BATTERY_THRESHOLD_CRITICAL: f32 = 0.1;

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

    /// Converts the BatteryCapacity to watt-hours.
    pub fn as_wh(self, microvolts: u64) -> f64 {
        match self {
            BatteryCapacity::MicroWattHours(uwh) => uwh as f64 / 1_000_000.0,
            BatteryCapacity::MicroAmpereHours(uah) => {
                // convert µV × µAh = pWh (picowatt-hours), then to watt-hours
                let capacity_pwh = microvolts * uah;
                capacity_pwh as f64 / 1e12_f64
            }
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
