use chrono::Local;

use crate::battery::features::{NUM_FEATURES, get_time_values};

pub const QUARTER_HOURS_IN_A_WEEK: usize = 24 * 7 * 4;

/// Recursive Least Squares (RLS) model for battery drain prediction.
///
/// Uses exponentially-weighted forgetting factor to adapt to changing
/// conditions. Predicts battery drain rate (watts) from 5 features.
#[derive(Debug, Clone)]
pub struct RlsModel {
    /// Weight vector.
    pub(super) weights: [f64; NUM_FEATURES],

    /// Inverse covariance matrix (stored as flattened row-major).
    pub(super) p_matrix: Vec<f64>,

    /// Forgetting factor (0.95-0.995). Lower = faster adaptation.
    pub(super) lambda: f64,

    /// Number of samples seen by quarter-hour of week.
    pub(super) sample_count: Vec<u32>,
}

impl RlsModel {
    /// Create a new RLS model.
    ///
    /// # Parameters
    /// - `lambda`: forgetting factor (0.95-0.995). lower = faster adaptation to
    ///   new patterns.
    /// - `initial_variance`: initial uncertainty (1.0-10.0). higher = faster
    ///   initial learning.
    pub fn new(lambda: f64, initial_variance: f64) -> Self {
        // initialize weights to zero
        let weights = [0.0; NUM_FEATURES];

        // initialize P matrix as identity × initial_variance
        let mut p_matrix = vec![0.0; NUM_FEATURES * NUM_FEATURES];
        for i in 0..NUM_FEATURES {
            p_matrix[i * NUM_FEATURES + i] = initial_variance;
        }

        Self {
            weights,
            p_matrix,
            lambda,
            sample_count: vec![0; QUARTER_HOURS_IN_A_WEEK],
        }
    }

    /// Update the model with a new observation.
    ///
    /// # Parameters
    /// - `features`: feature vector
    /// - `target`: observed battery drain rate (watts)
    pub fn update(&mut self, features: &[f64; NUM_FEATURES], target: f64) {
        // compute P × features
        let mut p_phi = [0.0; NUM_FEATURES];
        for (i, item) in p_phi.iter_mut().enumerate().take(NUM_FEATURES) {
            let mut sum = 0.0;
            for (j, feature) in features.iter().enumerate().take(NUM_FEATURES) {
                sum += self.p_matrix[i * NUM_FEATURES + j] * feature;
            }
            *item = sum;
        }

        // compute features^T × P × features
        let mut phi_p_phi = 0.0;
        for i in 0..NUM_FEATURES {
            phi_p_phi += features[i] * p_phi[i];
        }

        // compute gain: k = P × features / (lambda + features^T × P × features)
        let denominator = self.lambda + phi_p_phi;
        let mut gain = [0.0; NUM_FEATURES];
        for i in 0..NUM_FEATURES {
            gain[i] = p_phi[i] / denominator;
        }

        // compute prediction error
        let prediction = self.predict(features);
        let error = target - prediction;

        // update weights: w = w + k × error
        for (i, item) in gain.iter().enumerate() {
            self.weights[i] += item * error;
        }

        // update P matrix: P = (P - k × features^T × P) / lambda
        let mut new_p = vec![0.0; NUM_FEATURES * NUM_FEATURES];
        for i in 0..NUM_FEATURES {
            for j in 0..NUM_FEATURES {
                let mut kg_phi_p = 0.0;
                for (k, feature) in features.iter().enumerate() {
                    kg_phi_p += gain[i] * feature * self.p_matrix[k * NUM_FEATURES + j];
                }
                new_p[i * NUM_FEATURES + j] =
                    (self.p_matrix[i * NUM_FEATURES + j] - kg_phi_p) / self.lambda;
            }
        }
        self.p_matrix = new_p;

        let (_, day_of_week) = get_time_values(Local::now());
        let hour_of_week = day_of_week * 24.0;
        let quarter_hour_of_week = (hour_of_week * 4.0).floor() as usize;
        self.sample_count[quarter_hour_of_week] += 1;
    }

    /// Predict battery drain rate from features.
    pub fn predict(&self, features: &[f64; NUM_FEATURES]) -> f64 {
        let sum = self
            .weights
            .iter()
            .zip(features)
            .map(|(weight, feature)| weight * feature)
            .sum::<f64>();

        if sum < 0.0 {
            log::debug!("battery model is predicting a negative power rate: {sum}")
        }

        sum
    }

    /// Get the total number of samples seen.
    pub fn total_sample_count(&self) -> u32 {
        self.sample_count.iter().sum::<u32>()
    }

    /// Provides the confidence level of the model from 0.0-1.0.
    pub fn confidence(&self) -> f64 {
        let sum_of_confidence = self
            .sample_count
            .iter()
            .fold(0.0, |acc, n| acc + confidence_sigmoid(*n));

        (sum_of_confidence / MIN_QUARTER_HOURS_FOR_COMPLETE_CONFIDENCE as f64).clamp(0.0, 1.0)
    }
}

/// How many samples should be in a quarter-hour slot to be considered
/// confident.
const MIN_SAMPLES_PER_QUARTER_HOUR_FOR_CONFIDENCE: usize = 500;

/// The desired confidence we want to have at our minimum sample count.
const CONFIDENCE_AT_MIN_SAMPLES: f64 = 0.99;

/// How many quarter-hours should be covered for full model confidence.
/// Right now, this is the number of quarter-hours in a week, multiplied by the
/// fraction of typical waking hours in a day, assuming 8 hours of sleep.
const MIN_QUARTER_HOURS_FOR_COMPLETE_CONFIDENCE: usize = QUARTER_HOURS_IN_A_WEEK * 16 / 24;

fn confidence_sigmoid(samples: u32) -> f64 {
    let n = MIN_SAMPLES_PER_QUARTER_HOUR_FOR_CONFIDENCE as f64;
    let c = CONFIDENCE_AT_MIN_SAMPLES;
    let a = n / c - n;
    let x = samples as f64;

    (x / a) / (1. + (x / a))
}

impl Default for RlsModel {
    fn default() -> Self {
        // lambda=0.98: moderate adaptation speed
        // initial_variance=5.0: balanced initial learning rate
        Self::new(0.98, 5.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rls_learns_constant_drain() {
        let mut model = RlsModel::new(0.95, 10.0);

        // simulate constant 10W drain with constant features (7-feature vector)
        let features: [f64; NUM_FEATURES] = [1.0, 0.5, 0.3, 0.8, 0.2];
        let target = 10.0;

        // train for 50 iterations
        for _ in 0..50 {
            model.update(&features, target);
        }

        // prediction should converge to target
        let prediction = model.predict(&features);
        assert!(
            (prediction - target).abs() < 0.5,
            "prediction={}, target={}",
            prediction,
            target
        );
        assert_eq!(model.total_sample_count(), 50);
    }

    #[test]
    fn test_rls_adapts_to_change() {
        let mut model = RlsModel::new(0.90, 10.0); // faster adaptation

        let features: [f64; NUM_FEATURES] = [1.0, 0.5, 0.3, 0.8, 0.2];

        // train on 8W for 30 samples
        for _ in 0..30 {
            model.update(&features, 8.0);
        }

        let old_prediction = model.predict(&features);
        assert!((old_prediction - 8.0).abs() < 0.5);

        // sudden change to 15W
        for _ in 0..30 {
            model.update(&features, 15.0);
        }

        let new_prediction = model.predict(&features);
        assert!(
            (new_prediction - 15.0).abs() < 1.0,
            "new_prediction={}",
            new_prediction
        );
    }

    #[test]
    fn test_rls_multiple_feature_patterns() {
        let mut model = RlsModel::new(0.98, 5.0);

        // pattern 1: high power usage
        let features_high: [f64; NUM_FEATURES] = [1.0, 1.0, 0.9, 0.8, 0.7];
        // pattern 2: low power usage
        let features_low: [f64; NUM_FEATURES] = [0.1, 0.2, 0.1, 0.3, 0.2];

        // train on both patterns
        for _ in 0..25 {
            model.update(&features_high, 20.0);
            model.update(&features_low, 5.0);
        }

        // predictions should be reasonable for both
        let pred_high = model.predict(&features_high);
        let pred_low = model.predict(&features_low);

        assert!(pred_high > pred_low, "high={}, low={}", pred_high, pred_low);
        assert!((pred_high - 20.0).abs() < 3.0);
        assert!((pred_low - 5.0).abs() < 1.0);
    }

    #[test]
    fn test_no_negative_predictions() {
        let model = RlsModel::default();

        // all-zero features should give non-negative prediction
        let features = [0.0; NUM_FEATURES];
        let prediction = model.predict(&features);
        assert!(prediction >= 0.0);
    }
}
