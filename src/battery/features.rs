use chrono::{Datelike, Local, Timelike};

use super::sysfs::SysfsReading;

/// Extract 9 features from battery state for RLS model.
///
/// Power draw is intentionally excluded -- it is the prediction target, not
/// an input. Charging state is excluded because separate models are used for
/// charging and discharging.
///
/// Features:
///  0. hour_sin       -- fractional-hour daily cycle: sin(2π * hour_frac / 24)
///  1. hour_cos       -- fractional-hour daily cycle: cos(2π * hour_frac / 24)
///  2. day_sin        -- weekly cycle: sin(2π * day / 7)
///  3. day_cos        -- weekly cycle: cos(2π * day / 7)
///  4. week_hour_sin  -- 168-hour cycle: sin(2π * hour_of_week / 168)
///  5. week_hour_cos  -- 168-hour cycle: cos(2π * hour_of_week / 168)
///  6. percentage     -- charge_now / charge_full
///  7. percentage²    -- squared percentage for nonlinear curve fitting
///  8. percentage³    -- cubic percentage for CC/CV taper modelling
pub fn extract_features(reading: &SysfsReading) -> Option<[f64; 9]> {
    let now = Local::now();

    // fractional hour for sub-hour precision (e.g., 14.5 = 14:30)
    let hour_frac = now.hour() as f64 + now.minute() as f64 / 60.0 + now.second() as f64 / 3600.0;

    // features 0-1: daily cycle
    let hour_rad = 2.0 * std::f64::consts::PI * hour_frac / 24.0;
    let hour_sin = hour_rad.sin();
    let hour_cos = hour_rad.cos();

    // features 2-3: weekly cycle
    let day_of_week = now.weekday().num_days_from_monday() as f64 + hour_frac / 24.0;
    let dow_rad = 2.0 * std::f64::consts::PI * day_of_week / 7.0;
    let day_sin = dow_rad.sin();
    let day_cos = dow_rad.cos();

    // features 4-5: hour-of-week cycle (168 hours per week)
    // this distinguishes e.g. Monday 9am from Saturday 9am
    let hour_of_week = day_of_week * 24.0 + hour_frac;
    let how_rad = 2.0 * std::f64::consts::PI * hour_of_week / (24.0 * 7.0);
    let week_hour_sin = how_rad.sin();
    let week_hour_cos = how_rad.cos();

    // features 6-8: percentage and polynomial expansions
    let percentage = reading.percentage()?;
    let percentage_sq = percentage * percentage;
    let percentage_cu = percentage_sq * percentage;

    Some([
        hour_sin,
        hour_cos,
        day_sin,
        day_cos,
        week_hour_sin,
        week_hour_cos,
        percentage,
        percentage_sq,
        percentage_cu,
    ])
}

/// Project features forward in time for forward integration.
///
/// Only time features (indices 0-5) and the percentage polynomial (6-8) are
/// updated; the caller is responsible for computing the new percentage and
/// passing it in.
///
/// # Parameters
/// - `current_features`: current 9-element feature vector
/// - `seconds_ahead`: how many seconds into the future to project
/// - `new_percentage`: updated percentage based on energy consumed so far
pub fn project_features_forward(
    current_features: &[f64; 9],
    seconds_ahead: u64,
    new_percentage: f64,
) -> [f64; 9] {
    let mut projected = *current_features;

    let now = Local::now();
    let future = now + chrono::Duration::seconds(seconds_ahead as i64);

    // fractional hour for the future timestamp
    let hour_frac =
        future.hour() as f64 + future.minute() as f64 / 60.0 + future.second() as f64 / 3600.0;

    // features 0-1: daily cycle
    let hour_rad = 2.0 * std::f64::consts::PI * hour_frac / 24.0;
    projected[0] = hour_rad.sin();
    projected[1] = hour_rad.cos();

    // features 2-3: weekly cycle
    let day_of_week = future.weekday().num_days_from_monday() as f64;
    let dow_rad = 2.0 * std::f64::consts::PI * day_of_week / 7.0;
    projected[2] = dow_rad.sin();
    projected[3] = dow_rad.cos();

    // features 4-5: hour-of-week cycle
    let hour_of_week = day_of_week * 24.0 + hour_frac;
    let how_rad = 2.0 * std::f64::consts::PI * hour_of_week / 168.0;
    projected[4] = how_rad.sin();
    projected[5] = how_rad.cos();

    // features 6-8: percentage polynomial (updated by caller's energy accounting)
    let pct = new_percentage.clamp(0.0, 1.0);
    projected[6] = pct;
    projected[7] = pct * pct;
    projected[8] = pct * pct * pct;

    projected
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::battery::sysfs::ChargingStatus;

    fn make_reading() -> SysfsReading {
        SysfsReading {
            charge_now: Some(5_000_000),
            current_now: Some(2_000_000),
            voltage_now: Some(12_000_000),
            charge_full: Some(10_000_000),
            charge_full_design: Some(12_000_000),
            status: ChargingStatus::Discharging,
        }
    }

    #[test]
    fn test_feature_count() {
        let features = extract_features(&make_reading()).unwrap();
        assert_eq!(features.len(), NUM_FEATURES);
    }

    #[test]
    fn test_time_features_unit_circle() {
        let features = extract_features(&make_reading()).unwrap();

        // sin² + cos² must equal 1 for all three cyclical pairs
        let hour_sq = features[0].powi(2) + features[1].powi(2);
        assert!((hour_sq - 1.0).abs() < 1e-9, "hour cycle off unit circle");

        let day_sq = features[2].powi(2) + features[3].powi(2);
        assert!((day_sq - 1.0).abs() < 1e-9, "day cycle off unit circle");

        let how_sq = features[4].powi(2) + features[5].powi(2);
        assert!(
            (how_sq - 1.0).abs() < 1e-9,
            "week_hour cycle off unit circle"
        );
    }

    #[test]
    fn test_percentage_polynomial() {
        let features = extract_features(&make_reading()).unwrap();
        let pct = features[6];
        assert!((features[7] - pct * pct).abs() < 1e-9, "percentage² wrong");
        assert!(
            (features[8] - pct * pct * pct).abs() < 1e-9,
            "percentage³ wrong"
        );
    }

    #[test]
    fn test_project_features_forward_time_changes() {
        let current = extract_features(&make_reading()).unwrap();

        // project 6 hours forward -- time features must change
        let projected = project_features_forward(&current, 6 * 3600, 0.3);

        // at least one time feature should differ
        let time_changed = projected[0] != current[0]
            || projected[1] != current[1]
            || projected[2] != current[2]
            || projected[3] != current[3]
            || projected[4] != current[4]
            || projected[5] != current[5];
        assert!(time_changed, "no time features changed after 6h projection");
    }

    #[test]
    fn test_project_features_forward_percentage_updated() {
        let current = extract_features(&make_reading()).unwrap();
        let projected = project_features_forward(&current, 3600, 0.35);

        assert!((projected[6] - 0.35).abs() < 1e-9);
        assert!((projected[7] - 0.35_f64.powi(2)).abs() < 1e-9);
        assert!((projected[8] - 0.35_f64.powi(3)).abs() < 1e-9);
    }

    #[test]
    fn test_projected_time_features_on_unit_circle() {
        let current = extract_features(&make_reading()).unwrap();
        let projected = project_features_forward(&current, 13 * 3600, 0.5);

        let hour_sq = projected[0].powi(2) + projected[1].powi(2);
        assert!((hour_sq - 1.0).abs() < 1e-9);

        let day_sq = projected[2].powi(2) + projected[3].powi(2);
        assert!((day_sq - 1.0).abs() < 1e-9);

        let how_sq = projected[4].powi(2) + projected[5].powi(2);
        assert!((how_sq - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_missing_percentage_returns_none() {
        let reading = SysfsReading {
            charge_now: None,
            current_now: Some(1_000_000),
            voltage_now: Some(12_000_000),
            charge_full: None,
            charge_full_design: None,
            status: ChargingStatus::Discharging,
        };

        assert!(extract_features(&reading).is_none());
    }
}
