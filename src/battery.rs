use std::time::Duration;

use relm4::SharedState;

mod cpu;
mod features;
mod model;
mod persistence;
mod predictor;
mod sysfs;
mod watcher;

pub use cpu::read_cpu_load;
pub use features::extract_features;
pub use persistence::{load_predictor, save_predictor};
pub use predictor::BatteryPredictor;
pub use watcher::start_battery_watcher;

pub static BATTERY_STATE: SharedState<Option<BatteryState>> = SharedState::new();

/// Minimum confidence threshold to use smart prediction over kernel estimate.
pub const MIN_SMART_PREDICTION_CONFIDENCE: f32 = 0.3;

#[derive(Debug, Copy, Clone, PartialEq, Default)]
pub struct BatteryState {
    pub percentage: f32,
    pub charging: bool,
    pub time_remaining: Duration, // kernel/sysfs estimate (kept for reference)
    pub smart_time_remaining: Duration, // ml-enhanced estimate
    pub confidence: f32,          // 0.0-1.0, prediction confidence
}
