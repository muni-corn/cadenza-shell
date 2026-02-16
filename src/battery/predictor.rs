use std::time::Duration;

use super::{
    extract_features, features::project_features_forward, model::RlsModel, profile::UsageProfile,
    sysfs::SysfsReading,
};

/// Battery life predictor combining EWMA, RLS, and usage profile.
#[derive(Debug, Clone)]
pub struct BatteryPredictor {
    /// Recursive Least Squares model.
    pub(super) rls_model: RlsModel,
    /// Historical usage profile.
    pub(super) usage_profile: UsageProfile,
    /// Exponentially-weighted moving average of power draw.
    pub(super) ewma_power: Option<f64>,
    /// EWMA smoothing factor.
    pub(super) ewma_alpha: f64,
}

impl BatteryPredictor {
    pub fn new() -> Self {
        Self {
            rls_model: RlsModel::default(),
            usage_profile: UsageProfile::default(),
            ewma_power: None,
            ewma_alpha: 0.3, // moderate smoothing
        }
    }

    /// Update predictor with new battery reading.
    pub fn update(&mut self, reading: &SysfsReading) {
        // extract features and update models
        if let Some(features) = extract_features(reading) {
            let power = features[0]; // power_draw is first feature

            // update EWMA
            self.ewma_power = Some(match self.ewma_power {
                Some(prev) => self.ewma_alpha * power + (1.0 - self.ewma_alpha) * prev,
                None => power,
            });

            // update usage profile
            self.usage_profile.update(power);

            // update RLS model
            self.rls_model.update(&features, power);
        }
    }

    /// Predict time remaining until battery depletes.
    ///
    /// Uses tiered prediction strategy:
    /// 1. Forward integration (RLS + profile) - if RLS trained
    /// 2. Instantaneous RLS - if RLS trained
    /// 3. EWMA - if available
    /// 4. Profile average - fallback
    ///
    /// Returns (time_remaining, confidence).
    pub fn predict_time_remaining(&self, reading: &SysfsReading) -> Option<(Duration, f32)> {
        let features = extract_features(reading)?;

        let percentage = features[7];
        if percentage <= 0.01 {
            return Some((Duration::from_secs(0), 1.0)); // battery dead
        }

        // get battery capacity in watt-hours
        let capacity_wh = self.estimate_capacity_wh(reading)?;
        let remaining_wh = capacity_wh * percentage;

        // try tiered prediction
        if self.rls_model.is_trained() {
            // strategy 1: forward integration with RLS + profile
            if let Some((time, conf)) = self.predict_with_integration(&features, remaining_wh) {
                return Some((time, conf));
            }

            // strategy 2: instantaneous RLS
            let power = self.rls_model.predict(&features).max(0.5); // min 0.5W to avoid division by zero
            let hours = remaining_wh / power;
            let seconds = (hours * 3600.0) as u64;
            return Some((Duration::from_secs(seconds), 0.7)); // moderate confidence
        }

        // strategy 3: EWMA fallback
        if let Some(ewma) = self.ewma_power {
            let power = ewma.max(0.5);
            let hours = remaining_wh / power;
            let seconds = (hours * 3600.0) as u64;
            return Some((Duration::from_secs(seconds), 0.5)); // lower confidence
        }

        // strategy 4: profile fallback
        let power = self.usage_profile.get_current_power().max(0.5);
        let hours = remaining_wh / power;
        let seconds = (hours * 3600.0) as u64;
        let confidence = self.usage_profile.get_confidence() as f32 * 0.4; // very low confidence
        Some((Duration::from_secs(seconds), confidence))
    }

    /// Predict using forward time integration.
    ///
    /// Integrates predicted power draw over future time slots until battery
    /// depletes.
    fn predict_with_integration(
        &self,
        current_features: &[f64; 8],
        remaining_wh: f64,
    ) -> Option<(Duration, f32)> {
        const TIME_STEP: u64 = 900; // 15-minute steps
        const MAX_ITERATIONS: u32 = 4 * 24 * 7; // 1 week max

        let mut energy_remaining = remaining_wh;
        let mut total_seconds = 0u64;

        for i in 0..MAX_ITERATIONS {
            let seconds_ahead = total_seconds + TIME_STEP;

            // project features forward
            let future_features = project_features_forward(current_features, seconds_ahead);

            // predict power draw
            let predicted_power = self.rls_model.predict(&future_features).max(0.5);

            // calculate energy consumed in this time step
            let hours = TIME_STEP as f64 / 3600.0;
            let energy_consumed = predicted_power * hours;

            energy_remaining -= energy_consumed;
            total_seconds = seconds_ahead;

            if energy_remaining <= 0.0 {
                // battery depleted - interpolate for accuracy
                let overshoot = -energy_remaining;
                let fraction = 1.0 - (overshoot / energy_consumed);
                let final_seconds =
                    total_seconds - TIME_STEP + (TIME_STEP as f64 * fraction) as u64;

                // confidence based on profile and model maturity
                let profile_conf = self.usage_profile.get_confidence() as f32;
                let model_conf = (self.rls_model.sample_count() as f32 / 50.0).min(1.0);
                let confidence = (profile_conf * 0.5 + model_conf * 0.5) * 0.9; // 0.9 = integration confidence

                return Some((Duration::from_secs(final_seconds), confidence));
            }
        }

        // didn't converge within max time
        None
    }

    /// Estimate battery capacity in watt-hours.
    fn estimate_capacity_wh(&self, reading: &SysfsReading) -> Option<f64> {
        let charge_full = reading.charge_full? as f64; // µAh
        let voltage = reading.voltage_now? as f64; // µV

        // convert to watt-hours: (µAh × µV) / 1e12 = (Ah × V) = Wh
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

    #[test]
    fn test_predictor_update() {
        let mut predictor = BatteryPredictor::new();

        let reading = SysfsReading {
            charge_now: Some(5_000_000),
            current_now: Some(2_000_000),
            voltage_now: Some(12_000_000),
            charge_full: Some(10_000_000),
            charge_full_design: Some(10_000_000),
            status: ChargingStatus::Discharging,
        };

        assert!(predictor.ewma_power.is_none());

        predictor.update(&reading);

        // EWMA should be initialized
        assert!(predictor.ewma_power.is_some());
        assert!(predictor.ewma_power.unwrap() > 0.0);
    }

    #[test]
    fn test_predictor_time_remaining() {
        let mut predictor = BatteryPredictor::new();

        let reading = SysfsReading {
            charge_now: Some(5_000_000),   // 50% charged
            current_now: Some(1_000_000),  // 1A
            voltage_now: Some(12_000_000), // 12V
            charge_full: Some(10_000_000), // 10Ah
            charge_full_design: Some(10_000_000),
            status: ChargingStatus::Discharging,
        };

        // train predictor
        for _ in 0..30 {
            predictor.update(&reading);
        }

        let (time_remaining, confidence) = predictor.predict_time_remaining(&reading).unwrap();

        // should predict some reasonable time
        assert!(time_remaining.as_secs() > 0);
        assert!(time_remaining.as_secs() < 24 * 3600); // less than 24 hours

        // should have some confidence
        assert!(confidence > 0.0);
        assert!(confidence <= 1.0);
    }

    #[test]
    fn test_predictor_zero_battery() {
        let predictor = BatteryPredictor::new();

        let reading = SysfsReading {
            charge_now: Some(0), // 0% charged
            current_now: Some(1_000_000),
            voltage_now: Some(12_000_000),
            charge_full: Some(10_000_000),
            charge_full_design: Some(10_000_000),
            status: ChargingStatus::Discharging,
        };

        let (time_remaining, confidence) = predictor.predict_time_remaining(&reading).unwrap();

        assert_eq!(time_remaining.as_secs(), 0);
        assert_eq!(confidence, 1.0);
    }

    #[test]
    fn test_capacity_estimation() {
        let predictor = BatteryPredictor::new();

        let reading = SysfsReading {
            charge_now: Some(5_000_000),
            current_now: Some(1_000_000),
            voltage_now: Some(12_000_000), // 12V
            charge_full: Some(10_000_000), // 10Ah
            charge_full_design: Some(10_000_000),
            status: ChargingStatus::Discharging,
        };

        let capacity_wh = predictor.estimate_capacity_wh(&reading).unwrap();

        // 10Ah × 12V = 120Wh
        assert!((capacity_wh - 120.0).abs() < 0.1);
    }
}
