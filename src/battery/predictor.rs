use std::time::Duration;

use chrono::{DateTime, Local};

use super::{
    features::{extract_features, project_features_forward},
    model::RlsModel,
    sysfs::{BatteryCapacity, ChargingStatus, SysfsReading},
};
use crate::battery::features::NUM_FEATURES;

const EWMA_ALPHA: f64 = 0.3;

/// The minimum number of seconds that must pass before the predictor can be
/// updated.
const MIN_UPDATE_TIME: f32 = 5.0;

/// The all-encompassing battery life predictor combining EWMA and RLS.
///
/// This updates and maintains separate RLS models and EWMA accumulators for
/// charging and discharging. Charging and discharging have fundamentally
/// different physics and power magnitudes, so we've separated them.
#[derive(Debug, Clone)]
pub struct BatteryPredictor {
    /// RLS model for discharging (predicts power draw in watts).
    pub(super) rls_discharge: RlsModel,

    /// RLS model for charging (predicts charging power intake in watts).
    pub(super) rls_charge: RlsModel,

    /// EWMA of power draw while discharging (watts).
    pub(super) ewma_power_discharge: Option<f64>,

    /// EWMA of power intake while charging (watts).
    pub(super) ewma_power_charge: Option<f64>,

    /// EWMA-smoothed battery voltage in microvolts.
    ///
    /// Instantaneous voltage fluctuates with load; a heavily-smoothed value
    /// gives more stable Wh capacity estimates.
    pub(super) ewma_voltage: Option<f64>,

    /// The time at which the predictor was last updated with data.
    pub(super) last_update: DateTime<Local>,
}

impl BatteryPredictor {
    pub fn new() -> Self {
        Self {
            rls_discharge: RlsModel::default(),
            rls_charge: RlsModel::default(),
            ewma_power_discharge: None,
            ewma_power_charge: None,
            ewma_voltage: None,
            last_update: Local::now(),
        }
    }

    /// Update predictor with new battery reading.
    pub fn update(&mut self, reading: &SysfsReading) {
        let now = Local::now();

        if now.signed_duration_since(self.last_update).as_seconds_f32() < MIN_UPDATE_TIME {
            log::info!(
                "not updating battery predictor before {MIN_UPDATE_TIME} seconds have passed"
            );
            return;
        }

        // return early if there is no power reading
        let power_now = reading.power_watts();

        log::info!("updating battery predictor now");
        log::debug!("reading: {reading:?}");

        // select the EWMA accumulator for the current charging state
        // return early if the battery is not charging or discharging
        let (power_ewma, rls) = match reading.status {
            ChargingStatus::Charging => (&mut self.ewma_power_charge, &mut self.rls_charge),
            ChargingStatus::Discharging => {
                (&mut self.ewma_power_discharge, &mut self.rls_discharge)
            }
            _ => {
                log::warn!(
                    "battery is neither charging nor discharging; not updating prediction model"
                );
                return;
            }
        };

        self.last_update = Local::now();

        // update smoothed voltage
        let voltage_now = reading.voltage_now as f64;
        self.ewma_voltage = self
            .ewma_voltage
            .map(|previous_voltage| {
                EWMA_ALPHA * voltage_now + (1.0 - EWMA_ALPHA) * previous_voltage
            })
            .or(Some(voltage_now));

        // update EWMA for power
        *power_ewma = power_ewma
            .map(|previous_power| EWMA_ALPHA * power_now + (1.0 - EWMA_ALPHA) * previous_power)
            .or(Some(power_now));

        // extract features and train the appropriate model
        if let Some(features) = extract_features(reading) {
            rls.update(&features, power_now)
        }
    }

    /// Predict time remaining until battery depletes or charges to full.
    ///
    /// Dispatches to time-to-empty or time-to-full based on charging status.
    ///
    /// Returns `(duration, confidence)`, or `None` if no estimate is possible.
    pub fn predict_time_remaining(&self, reading: &SysfsReading) -> Option<(Duration, f64)> {
        let features = extract_features(reading)?;

        let percentage = features[4];

        let result = match reading.status {
            ChargingStatus::Charging => Some(self.predict_time_to_full(reading, &features)),
            ChargingStatus::Discharging => {
                if percentage <= 0.01 {
                    return Some((Duration::from_secs(0), 1.0));
                }
                Some(self.predict_time_to_empty(reading, &features))
            }
            _ => None,
        };

        if let Some((_, confidence)) = result {
            log::info!("prediction made with {:.3}% confidence", confidence * 100.);
        }

        result
    }

    /// Predict time until battery is empty (discharging).
    fn predict_time_to_empty(
        &self,
        reading: &SysfsReading,
        features: &[f64; NUM_FEATURES],
    ) -> (Duration, f64) {
        let percentage = features[4];
        let capacity_wh = self.estimate_capacity_wh(reading);
        let remaining_wh = capacity_wh * percentage;

        // get integrated predicted time
        let (predicted_time_remaining, rls_confidence) =
            self.predict_with_integration(features, remaining_wh, capacity_wh, false);

        // if no ewma right now, return just the integration prediction
        let Some(power) = self.ewma_power_discharge else {
            return (predicted_time_remaining, rls_confidence);
        };

        let ewma_secs_remaining = (remaining_wh / power) * 3600.0;

        // return a weighted average based on the model's confidence
        let weighted_secs_remaining = predicted_time_remaining.as_secs_f64() * rls_confidence
            + ewma_secs_remaining * (1.0 - rls_confidence);

        (
            Duration::from_secs_f64(weighted_secs_remaining),
            rls_confidence,
        )
    }

    /// Predict time until battery is full (charging).
    fn predict_time_to_full(
        &self,
        reading: &SysfsReading,
        features: &[f64; NUM_FEATURES],
    ) -> (Duration, f64) {
        let percentage = features[4];
        let capacity_wh = self.estimate_capacity_wh(reading);
        let remaining_wh = capacity_wh * percentage;
        let energy_to_full = capacity_wh - remaining_wh;

        if energy_to_full <= 0.0 {
            return (Duration::from_secs(0), 1.0);
        }

        // get integrated predicted time
        let (predicted_time_remaining, rls_confidence) =
            self.predict_with_integration(features, remaining_wh, capacity_wh, true);

        // if no ewma right now, return just the integration prediction
        let Some(power) = self.ewma_power_charge else {
            return (predicted_time_remaining, rls_confidence);
        };

        let ewma_secs_remaining = (energy_to_full / power) * 3600.0;

        // return a weighted average based on the model's confidence
        let weighted_secs_remaining = predicted_time_remaining.as_secs_f64() * rls_confidence
            + ewma_secs_remaining * (1.0 - rls_confidence);

        (
            Duration::from_secs_f64(weighted_secs_remaining),
            rls_confidence,
        )
    }

    /// Predict using forward time integration.
    ///
    /// Simulates energy flow in 15-minute steps until the battery depletes
    /// (discharging) or reaches full capacity (charging). Interpolates within
    /// the final step for sub-step accuracy.
    ///
    /// # Parameters
    /// - `current_features`: current 9-element feature vector
    /// - `remaining_wh`: watt-hours currently remaining
    /// - `capacity_wh`: total battery capacity in watt-hours
    /// - `charging`: true to integrate toward full, false toward empty
    fn predict_with_integration(
        &self,
        current_features: &[f64; NUM_FEATURES],
        remaining_wh: f64,
        capacity_wh: f64,
        charging: bool,
    ) -> (Duration, f64) {
        const TIME_STEP: u64 = 900; // 15-minute steps
        const MAX_ITERATIONS: u32 = 4 * 24 * 7; // 1 week max

        let rls = if charging {
            &self.rls_charge
        } else {
            &self.rls_discharge
        };

        let mut energy_remaining = remaining_wh;
        let mut total_seconds = 0u64;

        // count the number of times power prediction was negative
        let mut negative_predictions = 0;

        for _ in 0..MAX_ITERATIONS {
            total_seconds += TIME_STEP;

            let current_percent = (energy_remaining / capacity_wh).clamp(0.0, 1.0);
            let future_features =
                project_features_forward(current_features, total_seconds, current_percent);

            let raw_prediction = rls.predict(&future_features);
            if raw_prediction < 0.0 {
                negative_predictions += 1;
            }

            let predicted_power = raw_prediction.max(0.0);
            let hours = TIME_STEP as f64 / 3600.0;
            let energy_delta = predicted_power * hours;

            if charging {
                energy_remaining += energy_delta;
                if energy_remaining >= capacity_wh {
                    // interpolate within final step
                    let overshoot = energy_remaining - capacity_wh;
                    let fraction = if energy_delta > 0.0 {
                        1.0 - (overshoot / energy_delta)
                    } else {
                        0.0
                    };
                    let final_seconds =
                        total_seconds - TIME_STEP + (TIME_STEP as f64 * fraction) as u64;
                    let final_duration = Duration::from_secs(final_seconds);
                    let confidence = rls.confidence();

                    // warn about the number of negative predictions made
                    if negative_predictions > 0 {
                        log::warn!("rls model made {negative_predictions} negative predictions");
                    }

                    log::info!(
                        "rls model predicted {} hours and {} minutes remaining ({})",
                        final_seconds / 3600,
                        (final_seconds / 60) % 60,
                        (Local::now() + final_duration).format("%v %r")
                    );

                    return (final_duration, confidence);
                }
            } else {
                energy_remaining -= energy_delta;
                if energy_remaining <= 0.0 {
                    // interpolate within final step
                    let overshoot = -energy_remaining;
                    let fraction = if energy_delta > 0.0 {
                        1.0 - (overshoot / energy_delta)
                    } else {
                        0.0
                    };
                    let final_seconds =
                        total_seconds - TIME_STEP + (TIME_STEP as f64 * fraction) as u64;
                    let final_duration = Duration::from_secs(final_seconds);
                    let confidence = rls.confidence();

                    // warn about the number of negative predictions made
                    if negative_predictions > 0 {
                        log::warn!("rls model made {negative_predictions} negative predictions");
                    }

                    log::info!(
                        "rls model predicted {} hours and {} minutes remaining ({})",
                        final_seconds / 3600,
                        (final_seconds / 60) % 60,
                        (Local::now() + final_duration).format("%v %r")
                    );

                    return (final_duration, confidence);
                }
            }
        }

        // warn about the number of negative predictions made
        if negative_predictions > 0 {
            log::warn!(
                "rls model made {negative_predictions} negative predictions and never converged"
            );
        }

        (Duration::MAX, 0.0) // did not converge within 1 week
    }

    /// Estimate battery capacity in watt-hours from sysfs readings.
    ///
    /// Prefers the EWMA-smoothed voltage over the instantaneous reading to
    /// reduce noise from load-induced voltage sag. Falls back to instantaneous
    /// voltage if no smoothed value is available yet (first reading).
    fn estimate_capacity_wh(&self, reading: &SysfsReading) -> f64 {
        match reading.capacity_full {
            BatteryCapacity::MicroAmpereHours(u_ah) => {
                let u_ah = u_ah as f64;

                // prefer smoothed voltage; fall back to instantaneous if not yet warmed up
                let u_v = self.ewma_voltage.unwrap_or(reading.voltage_now as f64); // µV

                // (µAh × µV) / 1e12 = Wh
                (u_ah * u_v) / 1_000_000_000_000.0
            }
            BatteryCapacity::MicroWattHours(u_wh) => u_wh as f64 / 1_000_000.0,
        }
    }
}

impl Default for BatteryPredictor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::battery::sysfs::ChargingStatus;

    fn discharging_reading() -> SysfsReading {
        SysfsReading {
            current_now: 1_000_000,
            voltage_now: 12_000_000,
            capacity_now: BatteryCapacity::MicroAmpereHours(5_000_000),
            capacity_full: BatteryCapacity::MicroAmpereHours(10_000_000),
            status: ChargingStatus::Discharging,
        }
    }

    fn charging_reading() -> SysfsReading {
        SysfsReading {
            current_now: 2_000_000,
            voltage_now: 12_000_000,
            capacity_now: BatteryCapacity::MicroAmpereHours(5_000_000),
            capacity_full: BatteryCapacity::MicroAmpereHours(10_000_000),
            status: ChargingStatus::Charging,
        }
    }

    #[test]
    fn test_ewma_discharge_initialized_on_discharging_reading() {
        let mut predictor = BatteryPredictor::new();
        assert!(predictor.ewma_power_discharge.is_none());
        assert!(predictor.ewma_power_charge.is_none());

        predictor.update(&discharging_reading());

        assert!(predictor.ewma_power_discharge.is_some());
        assert!(predictor.ewma_power_discharge.unwrap() > 0.0);
        assert!(predictor.ewma_power_charge.is_none()); // charge EWMA must not be touched
    }

    #[test]
    fn test_ewma_charge_initialized_on_charging_reading() {
        let mut predictor = BatteryPredictor::new();
        predictor.update(&charging_reading());

        assert!(predictor.ewma_power_charge.is_some());
        assert!(predictor.ewma_power_charge.unwrap() > 0.0);
        assert!(predictor.ewma_power_discharge.is_none()); // discharge EWMA must not be touched
    }

    #[test]
    fn test_ewma_accumulators_are_independent() {
        let mut predictor = BatteryPredictor::new();

        for _ in 0..5 {
            predictor.update(&discharging_reading());
        }
        for _ in 0..5 {
            predictor.update(&charging_reading());
        }

        // charging power (2A × 12V = 24W) is higher than discharging (1A × 12V = 12W)
        let ewma_d = predictor.ewma_power_discharge.unwrap();
        let ewma_c = predictor.ewma_power_charge.unwrap();
        assert!(
            ewma_c > ewma_d,
            "ewma_charge={ewma_c:.2} should exceed ewma_discharge={ewma_d:.2}"
        );
    }

    #[test]
    fn test_discharge_and_charge_models_updated_independently() {
        let mut predictor = BatteryPredictor::new();

        for _ in 0..30 {
            predictor.update(&discharging_reading());
        }
        assert_eq!(predictor.rls_discharge.total_sample_count(), 30);
        assert_ne!(predictor.rls_charge.total_sample_count(), 30);

        for _ in 0..30 {
            predictor.update(&charging_reading());
        }
        assert_eq!(predictor.rls_discharge.total_sample_count(), 30);
        assert_eq!(predictor.rls_charge.total_sample_count(), 30);
    }

    #[test]
    fn test_time_to_empty_after_training() {
        let mut predictor = BatteryPredictor::new();
        let reading = discharging_reading();

        for _ in 0..30 {
            predictor.update(&reading);
        }

        let (time, confidence) = predictor.predict_time_remaining(&reading).unwrap();
        assert!(time.as_secs() > 0);
        assert!(time.as_secs() < 24 * 3600);
        assert!(confidence > 0.0 && confidence <= 1.0);
    }

    #[test]
    fn test_time_to_full_after_training() {
        let mut predictor = BatteryPredictor::new();
        let reading = charging_reading();

        for _ in 0..30 {
            predictor.update(&reading);
        }

        let (time, confidence) = predictor.predict_time_remaining(&reading).unwrap();
        assert!(time.as_secs() > 0);
        assert!(confidence > 0.0 && confidence <= 1.0);
    }

    #[test]
    fn test_full_battery_returns_none() {
        let predictor = BatteryPredictor::new();
        let reading = SysfsReading {
            current_now: 500_000,
            voltage_now: 12_000_000,
            capacity_now: BatteryCapacity::MicroAmpereHours(10_000_000),
            capacity_full: BatteryCapacity::MicroAmpereHours(10_000_000),
            status: ChargingStatus::Full,
        };

        assert!(predictor.predict_time_remaining(&reading).is_none());
    }

    #[test]
    fn test_zero_battery_returns_zero() {
        let predictor = BatteryPredictor::new();
        let reading = SysfsReading {
            current_now: 1_000_000,
            voltage_now: 12_000_000,
            capacity_now: BatteryCapacity::MicroAmpereHours(0),
            capacity_full: BatteryCapacity::MicroAmpereHours(10_000_000),
            status: ChargingStatus::Discharging,
        };

        let (time, confidence) = predictor.predict_time_remaining(&reading).unwrap();
        assert_eq!(time.as_secs(), 0);
        assert_eq!(confidence, 1.0);
    }

    #[test]
    fn test_capacity_estimation() {
        let predictor = BatteryPredictor::new();
        let reading = discharging_reading();
        // 10Ah × 12V = 120Wh
        let wh = predictor.estimate_capacity_wh(&reading);
        assert!((wh - 120.0).abs() < 0.1);
    }
}
