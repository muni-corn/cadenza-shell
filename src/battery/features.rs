use chrono::{Datelike, Local, Timelike};

use super::sysfs::SysfsReading;

/// Extract 12 context-only features from battery state for RLS model.
///
/// Power draw is intentionally excluded -- it is the prediction target, not
/// an input. Charging state is excluded because separate models are used for
/// charging and discharging.
///
/// Features:
///  0. hour_sin        -- fractional-hour daily cycle: sin(2π * hour_frac / 24)
///  1. hour_cos        -- fractional-hour daily cycle: cos(2π * hour_frac / 24)
///  2. day_sin         -- weekly cycle: sin(2π * day / 7)
///  3. day_cos         -- weekly cycle: cos(2π * day / 7)
///  4. week_hour_sin   -- 168-hour cycle: sin(2π * hour_of_week / 168)
///  5. week_hour_cos   -- 168-hour cycle: cos(2π * hour_of_week / 168)
///  6. battery_health  -- charge_full / charge_full_design
///  7. percentage      -- charge_now / charge_full
///  8. percentage²     -- squared percentage for nonlinear curve fitting
///  9. percentage³     -- cubic percentage for CC/CV taper modelling
/// 10. cpu_load        -- 1-min load average normalized by logical CPU count
/// 11. brightness      -- backlight level, 0.0-1.0 (0.5 if unavailable)
pub fn extract_features(
    reading: &SysfsReading,
    cpu_load: f64,
    brightness: f64,
) -> Option<[f64; 12]> {
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

    // feature 6: battery health (degradation factor)
    let battery_health = reading.battery_health().unwrap_or(1.0);

    // features 7-9: percentage and polynomial expansions
    let percentage = reading.percentage()?;
    let percentage_sq = percentage * percentage;
    let percentage_cu = percentage_sq * percentage;

    // feature 10: cpu load (already normalized 0.0-1.0 by caller)
    let cpu = cpu_load.clamp(0.0, 1.0);

    // feature 11: brightness (already normalized 0.0-1.0 by caller)
    let bri = brightness.clamp(0.0, 1.0);

    Some([
        hour_sin,
        hour_cos,
        day_sin,
        day_cos,
        week_hour_sin,
        week_hour_cos,
        battery_health,
        percentage,
        percentage_sq,
        percentage_cu,
        cpu,
        bri,
    ])
}

/// Project features forward in time for forward integration.
///
/// Only time features (indices 0-5) and the percentage polynomial (7-9) are
/// updated; the caller is responsible for computing the new percentage and
/// passing it in. Battery health, cpu_load, and brightness are held constant
/// because we cannot predict their future values.
///
/// # Parameters
/// - `current_features`: current 12-element feature vector
/// - `seconds_ahead`: how many seconds into the future to project
/// - `new_percentage`: updated percentage based on energy consumed so far
pub fn project_features_forward(
    current_features: &[f64; 12],
    seconds_ahead: u64,
    new_percentage: f64,
) -> [f64; 12] {
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

    // features 7-9: percentage polynomial (updated by caller's energy accounting)
    let pct = new_percentage.clamp(0.0, 1.0);
    projected[7] = pct;
    projected[8] = pct * pct;
    projected[9] = pct * pct * pct;

    // features 6 (battery_health), 10 (cpu_load), 11 (brightness) stay constant

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
        let features = extract_features(&make_reading(), 0.5, 0.8).unwrap();
        assert_eq!(features.len(), 12);
    }

    #[test]
    fn test_time_features_unit_circle() {
        let features = extract_features(&make_reading(), 0.0, 0.0).unwrap();

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
    fn test_battery_health_feature() {
        let features = extract_features(&make_reading(), 0.0, 0.0).unwrap();
        // charge_full=10Ah, charge_full_design=12Ah => health ≈ 0.833
        assert!((features[6] - 10.0 / 12.0).abs() < 1e-3);
    }

    #[test]
    fn test_percentage_polynomial() {
        let features = extract_features(&make_reading(), 0.0, 0.0).unwrap();
        let pct = features[7];
        assert!((features[8] - pct * pct).abs() < 1e-9, "percentage² wrong");
        assert!(
            (features[9] - pct * pct * pct).abs() < 1e-9,
            "percentage³ wrong"
        );
    }

    #[test]
    fn test_cpu_and_brightness_clamped() {
        // values above 1.0 should be clamped
        let features = extract_features(&make_reading(), 3.5, 1.2).unwrap();
        assert_eq!(features[10], 1.0);
        assert_eq!(features[11], 1.0);

        // values below 0.0 should be clamped
        let features2 = extract_features(&make_reading(), -0.1, -0.5).unwrap();
        assert_eq!(features2[10], 0.0);
        assert_eq!(features2[11], 0.0);
    }

    #[test]
    fn test_project_features_forward_time_changes() {
        let current = extract_features(&make_reading(), 0.4, 0.7).unwrap();

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
        let current = extract_features(&make_reading(), 0.4, 0.7).unwrap();
        let projected = project_features_forward(&current, 3600, 0.35);

        assert!((projected[7] - 0.35).abs() < 1e-9);
        assert!((projected[8] - 0.35_f64.powi(2)).abs() < 1e-9);
        assert!((projected[9] - 0.35_f64.powi(3)).abs() < 1e-9);
    }

    #[test]
    fn test_project_features_static_fields_unchanged() {
        let current = extract_features(&make_reading(), 0.4, 0.7).unwrap();
        let projected = project_features_forward(&current, 3600, 0.45);

        // battery_health (6), cpu_load (10), brightness (11) must be unchanged
        assert_eq!(projected[6], current[6]);
        assert_eq!(projected[10], current[10]);
        assert_eq!(projected[11], current[11]);
    }

    #[test]
    fn test_projected_time_features_on_unit_circle() {
        let current = extract_features(&make_reading(), 0.2, 0.5).unwrap();
        let projected = project_features_forward(&current, 13 * 3600, 0.5);

        let hour_sq = projected[0].powi(2) + projected[1].powi(2);
        assert!((hour_sq - 1.0).abs() < 1e-9);

        let day_sq = projected[2].powi(2) + projected[3].powi(2);
        assert!((day_sq - 1.0).abs() < 1e-9);

        let how_sq = projected[4].powi(2) + projected[5].powi(2);
        assert!((how_sq - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_missing_data_fallbacks() {
        let reading = SysfsReading {
            charge_now: None,
            current_now: Some(1_000_000),
            voltage_now: Some(12_000_000),
            charge_full: None,
            charge_full_design: None,
            status: ChargingStatus::Discharging,
        };

        let features = extract_features(&reading, 0.0, 0.5).unwrap();
        assert_eq!(features[6], 1.0); // battery_health default
        assert_eq!(features[7], 0.5); // percentage default
    }
}
