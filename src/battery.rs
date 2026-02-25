use std::time::Duration;

use relm4::SharedState;

mod features;
mod history;
mod model;
mod persistence;
mod predictor;
mod sysfs;
mod watcher;

pub use persistence::{load_predictor, save_predictor};
pub use predictor::BatteryPredictor;
pub use watcher::start_battery_watcher;

pub static BATTERY_STATE: SharedState<Option<BatteryState>> = SharedState::new();

#[derive(Debug, Copy, Clone, PartialEq, Default)]
pub struct BatteryState {
    pub percentage: f32,
    pub charging: bool,
    pub time_remaining: Duration,
}

#[derive(Clone, Copy, Debug)]
pub enum BatteryCapacity {
    /// µAh
    MicroAmpereHours(u64),

    /// µWh
    MicroWattHours(u64),
}

impl BatteryCapacity {
    pub fn div(self, rhs: Self) -> Option<f64> {
        match (self, rhs) {
            (Self::MicroAmpereHours(l), Self::MicroAmpereHours(r))
            | (Self::MicroWattHours(l), Self::MicroWattHours(r)) => Some(l as f64 / r as f64),
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
