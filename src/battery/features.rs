use chrono::{Datelike, Local, Timelike};

use super::sysfs::{ChargingStatus, SysfsReading};

/// Extract 8 features from battery state for RLS model.
///
/// Features:
/// 0. power_draw_watts (from sysfs current_now × voltage_now)
/// 1. hour_sin (cyclical encoding)
/// 2. hour_cos (cyclical encoding)
/// 3. day_of_week_sin (0=Monday, cyclical encoding)
/// 4. day_of_week_cos (cyclical encoding)
/// 5. battery_health (charge_full / charge_full_design)
/// 6. is_charging (1.0 if charging, 0.0 otherwise)
/// 7. percentage (charge_now / charge_full)
pub fn extract_features(reading: &SysfsReading) -> Option<[f64; 8]> {
    let now = Local::now();

    // feature 0: power draw in watts
    let power_draw = reading.power_watts()?;

    // features 1-2: hour cyclical encoding (0-23 hours)
    let hour = now.hour() as f64;
    let hour_rad = 2.0 * std::f64::consts::PI * hour / 24.0;
    let hour_sin = hour_rad.sin();
    let hour_cos = hour_rad.cos();

    // features 3-4: day of week cyclical encoding (0=Monday, 6=Sunday)
    let day_of_week = now.weekday().num_days_from_monday() as f64;
    let dow_rad = 2.0 * std::f64::consts::PI * day_of_week / 7.0;
    let day_sin = dow_rad.sin();
    let day_cos = dow_rad.cos();

    // feature 5: battery health
    let battery_health = reading.battery_health().unwrap_or(1.0);

    // feature 6: charging state
    let is_charging = match reading.status {
        ChargingStatus::Charging => 1.0,
        _ => 0.0,
    };

    // feature 7: percentage
    let percentage = reading.percentage().unwrap_or(0.5);

    Some([
        power_draw,
        hour_sin,
        hour_cos,
        day_sin,
        day_cos,
        battery_health,
        is_charging,
        percentage,
    ])
}

/// Project features forward in time (for prediction).
///
/// When predicting future battery life, we need to estimate what features
/// will look like at a future timestamp. Time features change based on the
/// future time. Other features (power_draw, battery_health, is_charging) stay
/// constant as we can't predict their future values. Note: percentage is kept
/// constant here, but should be updated by the caller based on energy consumed.
///
/// # Parameters
/// - `current_features`: current 8-element feature vector
/// - `seconds_ahead`: how many seconds into the future to project
pub fn project_features_forward(current_features: &[f64; 8], seconds_ahead: u64) -> [f64; 8] {
    let mut projected = *current_features;

    // get current time
    let now = Local::now();
    let future = now + chrono::Duration::seconds(seconds_ahead as i64);

    // update time features (indices 1-4)
    let hour = future.hour() as f64;
    let hour_rad = 2.0 * std::f64::consts::PI * hour / 24.0;
    projected[1] = hour_rad.sin();
    projected[2] = hour_rad.cos();

    let day_of_week = future.weekday().num_days_from_monday() as f64;
    let dow_rad = 2.0 * std::f64::consts::PI * day_of_week / 7.0;
    projected[3] = dow_rad.sin();
    projected[4] = dow_rad.cos();

    // power_draw (0), battery_health (5), is_charging (6), percentage (7) stay same
    projected
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_feature_extraction() {
        let reading = SysfsReading {
            charge_now: Some(5_000_000),          // 5 Ah
            current_now: Some(2_000_000),         // 2 A
            voltage_now: Some(12_000_000),        // 12 V
            charge_full: Some(10_000_000),        // 10 Ah
            charge_full_design: Some(12_000_000), // 12 Ah
            status: ChargingStatus::Discharging,
        };

        let features = extract_features(&reading).unwrap();

        // power: 12V × 2A = 24W
        assert!((features[0] - 24.0).abs() < 0.01);

        // hour_sin and hour_cos should be valid
        assert!(features[1].abs() <= 1.0);
        assert!(features[2].abs() <= 1.0);

        // day_sin and day_cos should be valid
        assert!(features[3].abs() <= 1.0);
        assert!(features[4].abs() <= 1.0);

        // battery_health: 10/12 ≈ 0.833
        assert!((features[5] - 0.833).abs() < 0.01);

        // is_charging: 0.0 (discharging)
        assert_eq!(features[6], 0.0);

        // percentage: 5/10 = 0.5
        assert!((features[7] - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_feature_extraction_charging() {
        let reading = SysfsReading {
            charge_now: Some(8_000_000),
            current_now: Some(1_500_000),
            voltage_now: Some(12_000_000),
            charge_full: Some(10_000_000),
            charge_full_design: Some(10_000_000),
            status: ChargingStatus::Charging,
        };

        let features = extract_features(&reading).unwrap();

        // is_charging: 1.0
        assert_eq!(features[6], 1.0);

        // battery_health: 1.0 (full/design both 10Ah)
        assert!((features[5] - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_cyclical_encoding_properties() {
        // at midnight, hour_sin should be ~0 and hour_cos should be ~1
        let reading = SysfsReading {
            charge_now: Some(5_000_000),
            current_now: Some(1_000_000),
            voltage_now: Some(12_000_000),
            charge_full: Some(10_000_000),
            charge_full_design: Some(10_000_000),
            status: ChargingStatus::Discharging,
        };

        let features = extract_features(&reading).unwrap();

        // verify sin² + cos² = 1 for hour encoding
        let hour_norm_sq = features[1].powi(2) + features[2].powi(2);
        assert!((hour_norm_sq - 1.0).abs() < 0.001);

        // verify sin² + cos² = 1 for day encoding
        let day_norm_sq = features[3].powi(2) + features[4].powi(2);
        assert!((day_norm_sq - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_project_features_forward() {
        let current = [
            15.0,  // power_draw
            0.5,   // hour_sin
            0.866, // hour_cos
            0.0,   // day_sin
            1.0,   // day_cos
            0.9,   // battery_health
            0.0,   // is_charging
            0.7,   // percentage
        ];

        // project 1 hour forward (3600 seconds)
        let projected = project_features_forward(&current, 3600);

        // time features should change
        assert_ne!(projected[1], current[1]);
        assert_ne!(projected[2], current[2]);

        // other features should stay same
        assert_eq!(projected[0], current[0]); // power_draw
        assert_eq!(projected[5], current[5]); // battery_health
        assert_eq!(projected[6], current[6]); // is_charging
        assert_eq!(projected[7], current[7]); // percentage

        // verify cyclical encoding still valid
        let hour_norm_sq = projected[1].powi(2) + projected[2].powi(2);
        assert!((hour_norm_sq - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_missing_data_handling() {
        // reading with missing optional fields
        let reading = SysfsReading {
            charge_now: None,
            current_now: Some(1_000_000),
            voltage_now: Some(12_000_000),
            charge_full: None,
            charge_full_design: None,
            status: ChargingStatus::Discharging,
        };

        let features = extract_features(&reading).unwrap();

        // should fall back to defaults
        assert_eq!(features[5], 1.0); // battery_health default
        assert_eq!(features[7], 0.5); // percentage default
    }
}
