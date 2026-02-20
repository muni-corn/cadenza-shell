use std::time::Duration;

use super::{
    features::{extract_features, project_features_forward},
    model::RlsModel,
    sysfs::{ChargingStatus, SysfsReading},
};
use crate::battery::features::NUM_FEATURES;

/// Battery life predictor combining EWMA and RLS.
///
/// Maintains separate RLS models and EWMA accumulators for charging and
/// discharging, since the two processes have fundamentally different physics
/// and power magnitudes.
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
    /// EWMA smoothing factor.
    pub(super) ewma_alpha: f64,
    /// EWMA-smoothed battery voltage in microvolts.
    ///
    /// Instantaneous voltage fluctuates with load; a heavily-smoothed value
    /// gives more stable Wh capacity estimates.
    pub(super) ewma_voltage: Option<f64>,
}

impl BatteryPredictor {
    pub fn new() -> Self {
        Self {
            rls_discharge: RlsModel::default(),
            rls_charge: RlsModel::default(),
            ewma_power_discharge: None,
            ewma_power_charge: None,
            ewma_alpha: 0.3,
            ewma_voltage: None,
        }
    }

    /// Update predictor with new battery reading.
    pub fn update(&mut self, reading: &SysfsReading) {
        let power = match reading.power_watts() {
            Some(p) => p,
            None => return,
        };

        // update smoothed voltage (alpha=0.1 for heavy smoothing; voltage
        // fluctuates with load so we want a stable long-run average)
        const VOLTAGE_ALPHA: f64 = 0.1;
        if let Some(v) = reading.voltage_now {
            let v = v as f64;
            self.ewma_voltage = Some(match self.ewma_voltage {
                Some(prev) => VOLTAGE_ALPHA * v + (1.0 - VOLTAGE_ALPHA) * prev,
                None => v,
            });
        }

        // select the EWMA accumulator for the current charging state
        let ewma = match reading.status {
            ChargingStatus::Charging => &mut self.ewma_power_charge,
            _ => &mut self.ewma_power_discharge,
        };

        // outlier detection: if the new reading is more than 3× the current
        // EWMA, it is likely a transient spike. apply a dampened alpha so the
        // EWMA moves only slightly, and skip the RLS update entirely to avoid
        // corrupting the model weights with a single anomalous sample.
        const OUTLIER_THRESHOLD: f64 = 3.0;
        const OUTLIER_ALPHA: f64 = 0.05; // very slow incorporation for spikes

        let is_outlier = ewma.is_some_and(|prev| prev > 0.0 && power > prev * OUTLIER_THRESHOLD);

        // update EWMA (always, but with dampened alpha for outliers)
        let alpha = if is_outlier {
            OUTLIER_ALPHA
        } else {
            self.ewma_alpha
        };
        *ewma = Some(match *ewma {
            Some(prev) => alpha * power + (1.0 - alpha) * prev,
            None => power,
        });

        // skip RLS update for outliers to protect model weights
        if is_outlier {
            log::debug!(
                "battery: skipping RLS update for outlier power reading ({:.1}W)",
                power
            );
            return;
        }

        // extract features and train the appropriate model
        if let Some(features) = extract_features(reading) {
            match reading.status {
                ChargingStatus::Charging => self.rls_charge.update(&features, power),
                _ => self.rls_discharge.update(&features, power),
            }
        }
    }

    /// Predict time remaining until battery depletes or charges to full.
    ///
    /// Dispatches to time-to-empty or time-to-full based on charging status.
    ///
    /// Returns `(duration, confidence)`, or `None` if no estimate is possible.
    pub fn predict_time_remaining(&self, reading: &SysfsReading) -> Option<(Duration, f32)> {
        let features = extract_features(reading)?;

        let percentage = features[6];

        match reading.status {
            ChargingStatus::Charging => self.predict_time_to_full(reading, &features),
            ChargingStatus::Full => Some((Duration::from_secs(0), 1.0)),
            _ => {
                if percentage <= 0.01 {
                    return Some((Duration::from_secs(0), 1.0));
                }
                self.predict_time_to_empty(reading, &features)
            }
        }
    }

    /// Predict time until battery is empty (discharging).
    fn predict_time_to_empty(
        &self,
        reading: &SysfsReading,
        features: &[f64; NUM_FEATURES],
    ) -> Option<(Duration, f32)> {
        let percentage = features[6];
        let capacity_wh = self.estimate_capacity_wh(reading)?;
        let remaining_wh = capacity_wh * percentage;

        if self.rls_discharge.is_trained() {
            // strategy 1: forward integration with discharge RLS model
            if let Some(result) =
                self.predict_with_integration(features, remaining_wh, capacity_wh, false)
            {
                return Some(result);
            }

            // strategy 2: instantaneous RLS
            let power = self.rls_discharge.predict(features).max(0.5);
            let seconds = ((remaining_wh / power) * 3600.0) as u64;
            return Some((Duration::from_secs(seconds), 0.7));
        }

        // strategy 3: discharge EWMA fallback
        if let Some(ewma) = self.ewma_power_discharge {
            let power = ewma.max(0.5);
            let seconds = ((remaining_wh / power) * 3600.0) as u64;
            return Some((Duration::from_secs(seconds), 0.5));
        }

        None
    }

    /// Predict time until battery is full (charging).
    fn predict_time_to_full(
        &self,
        reading: &SysfsReading,
        features: &[f64; NUM_FEATURES],
    ) -> Option<(Duration, f32)> {
        let percentage = features[6];
        let capacity_wh = self.estimate_capacity_wh(reading)?;
        let remaining_wh = capacity_wh * percentage;
        let energy_to_full = capacity_wh - remaining_wh;

        if energy_to_full <= 0.0 {
            return Some((Duration::from_secs(0), 1.0));
        }

        if self.rls_charge.is_trained() {
            // strategy 1: forward integration with charge RLS model
            if let Some(result) =
                self.predict_with_integration(features, remaining_wh, capacity_wh, true)
            {
                return Some(result);
            }

            // strategy 2: instantaneous RLS
            let power = self.rls_charge.predict(features).max(0.5);
            let seconds = ((energy_to_full / power) * 3600.0) as u64;
            return Some((Duration::from_secs(seconds), 0.7));
        }

        // strategy 3: charge EWMA fallback
        if let Some(ewma) = self.ewma_power_charge {
            let power = ewma.max(0.5);
            let seconds = ((energy_to_full / power) * 3600.0) as u64;
            return Some((Duration::from_secs(seconds), 0.5));
        }

        None
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
    ) -> Option<(Duration, f32)> {
        const TIME_STEP: u64 = 900; // 15-minute steps
        const MAX_ITERATIONS: u32 = 4 * 24 * 7; // 1 week max

        let rls = if charging {
            &self.rls_charge
        } else {
            &self.rls_discharge
        };

        let mut energy_remaining = remaining_wh;
        let mut total_seconds = 0u64;

        for _ in 0..MAX_ITERATIONS {
            total_seconds += TIME_STEP;

            let current_pct = (energy_remaining / capacity_wh).clamp(0.0, 1.0);
            let future_features =
                project_features_forward(current_features, total_seconds, current_pct);

            let predicted_power = rls.predict(&future_features).max(0.5);
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
                    let confidence = self.integration_confidence(rls);
                    return Some((Duration::from_secs(final_seconds), confidence));
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
                    let confidence = self.integration_confidence(rls);
                    return Some((Duration::from_secs(final_seconds), confidence));
                }
            }
        }

        None // did not converge within 1 week
    }

    /// Confidence score for the forward integration result.
    ///
    /// Saturates at 1.0 after 50 training samples. The 0.9 factor
    /// reflects that integration compounds prediction errors over time.
    fn integration_confidence(&self, rls: &RlsModel) -> f32 {
        let model_conf = (rls.sample_count() as f32 / 50.0).min(1.0);
        model_conf * 0.9
    }

    /// Estimate battery capacity in watt-hours from sysfs readings.
    ///
    /// Prefers the EWMA-smoothed voltage over the instantaneous reading to
    /// reduce noise from load-induced voltage sag. Falls back to instantaneous
    /// voltage if no smoothed value is available yet (first reading).
    fn estimate_capacity_wh(&self, reading: &SysfsReading) -> Option<f64> {
        let charge_full = reading.charge_full? as f64; // µAh

        // prefer smoothed voltage; fall back to instantaneous if not yet warmed up
        let voltage = self
            .ewma_voltage
            .or_else(|| reading.voltage_now.map(|v| v as f64))?; // µV

        // (µAh × µV) / 1e12 = Wh
        let wh = (charge_full * voltage) / 1_000_000_000_000.0;
        Some(wh)
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
            charge_now: Some(5_000_000),
            current_now: Some(1_000_000),
            voltage_now: Some(12_000_000),
            charge_full: Some(10_000_000),
            charge_full_design: Some(10_000_000),
            status: ChargingStatus::Discharging,
        }
    }

    fn charging_reading() -> SysfsReading {
        SysfsReading {
            charge_now: Some(5_000_000),
            current_now: Some(2_000_000),
            voltage_now: Some(12_000_000),
            charge_full: Some(10_000_000),
            charge_full_design: Some(10_000_000),
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
        assert!(predictor.rls_discharge.is_trained());
        assert!(!predictor.rls_charge.is_trained());

        for _ in 0..30 {
            predictor.update(&charging_reading());
        }
        assert!(predictor.rls_charge.is_trained());
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
    fn test_full_battery_returns_zero() {
        let predictor = BatteryPredictor::new();
        let reading = SysfsReading {
            charge_now: Some(10_000_000),
            current_now: Some(500_000),
            voltage_now: Some(12_000_000),
            charge_full: Some(10_000_000),
            charge_full_design: Some(10_000_000),
            status: ChargingStatus::Full,
        };

        let (time, confidence) = predictor.predict_time_remaining(&reading).unwrap();
        assert_eq!(time.as_secs(), 0);
        assert_eq!(confidence, 1.0);
    }

    #[test]
    fn test_zero_battery_returns_zero() {
        let predictor = BatteryPredictor::new();
        let reading = SysfsReading {
            charge_now: Some(0),
            current_now: Some(1_000_000),
            voltage_now: Some(12_000_000),
            charge_full: Some(10_000_000),
            charge_full_design: Some(10_000_000),
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
        let wh = predictor.estimate_capacity_wh(&reading).unwrap();
        assert!((wh - 120.0).abs() < 0.1);
    }
}
