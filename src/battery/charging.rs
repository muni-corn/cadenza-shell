//! CC/CV charging phase detection and time-to-full prediction.
//!
//! Provides a stub that currently delegates to the coefficient-based model
//! from [`super::history`]. Subsequent commits will replace this with full
//! CC-CV intelligence.

use std::time::Duration;

/// Predict the time until the battery is full using the legacy linear-taper
/// coefficient model.
///
/// # Parameters
/// - `percentage_now` – current state of charge as a fraction `[0, 1]`.
/// - `wh_capacity` – full battery capacity in watt-hours.
/// - `charging_coefficient` – learned slope: `power ≈ coefficient × (1 − soc)`.
///
/// Returns [`Duration::MAX`] when the battery is already full or no
/// coefficient has been learned yet.
pub fn predict_time_to_full(
    percentage_now: f64,
    wh_capacity: f64,
    charging_coefficient: f64,
) -> Duration {
    if percentage_now >= 1.0 || charging_coefficient == 0.0 {
        return Duration::MAX;
    }
    let estimated_power = charging_coefficient * (1.0 - percentage_now);
    let wh_to_go = wh_capacity * (1.0 - percentage_now);
    let hours_to_full = wh_to_go / estimated_power;
    Duration::from_secs_f64(hours_to_full * 3600.0)
}
