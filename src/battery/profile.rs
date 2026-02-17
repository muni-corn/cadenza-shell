use chrono::{Datelike, Local, Timelike};

/// Historical usage profile tracking average power draw per time slot.
///
/// Divides week into 336 slots (48 half-hours × 7 days).
/// Uses exponential moving average to adapt to changing patterns.
#[derive(Debug, Clone)]
pub struct UsageProfile {
    /// Average power draw (watts) for each 30-min slot (336 total).
    /// Index = (day_of_week * 48) + (hour * 2) + (minute >= 30 ? 1 : 0)
    pub(super) slots: Vec<f64>,
    /// Sample counts for each slot (for confidence).
    pub(super) counts: Vec<u32>,
    /// Smoothing factor for EWMA (0.0-1.0).
    pub(super) alpha: f64,
}

pub const NUM_PROFILE_SLOTS_PER_DAY: usize = 48; // 48 half-hour slots
pub const NUM_USAGE_PROFILE_SLOTS: usize = NUM_PROFILE_SLOTS_PER_DAY * 7; // 48 half-hours slots × 7 days

impl UsageProfile {
    /// Create a new usage profile.
    ///
    /// # Parameters
    /// - `alpha`: smoothing factor for EWMA (0.05-0.2). lower = slower
    ///   adaptation.
    /// - `default_power`: initial power estimate for all slots (watts).
    pub fn new(alpha: f64, default_power: f64) -> Self {
        Self {
            slots: vec![default_power; NUM_USAGE_PROFILE_SLOTS],
            counts: vec![0; NUM_USAGE_PROFILE_SLOTS],
            alpha,
        }
    }

    /// Update the profile with a new power draw observation.
    pub fn update(&mut self, power_draw: f64) {
        let slot_idx = Self::current_slot_index();

        // exponential moving average: new = alpha × observed + (1 - alpha) × old
        self.slots[slot_idx] = self.alpha * power_draw + (1.0 - self.alpha) * self.slots[slot_idx];
        self.counts[slot_idx] = self.counts[slot_idx].saturating_add(1);
    }

    /// Get average power draw for the current time slot.
    pub fn get_current_power(&self) -> f64 {
        let slot_idx = Self::current_slot_index();
        self.slots[slot_idx]
    }

    /// Get average power draw for a future time.
    pub fn get_power_at(&self, seconds_ahead: u64) -> f64 {
        let now = Local::now();
        let future = now + chrono::Duration::seconds(seconds_ahead as i64);

        let slot_idx = Self::slot_index_for_time(
            future.weekday().num_days_from_monday() as usize,
            future.hour() as usize,
            future.minute() as usize,
        );

        self.slots[slot_idx]
    }

    /// Get confidence for current slot (0.0-1.0).
    /// Confidence increases with sample count, saturating at 50 samples.
    pub fn get_confidence(&self) -> f64 {
        let slot_idx = Self::current_slot_index();
        let count = self.counts[slot_idx];
        (count as f64 / 50.0).min(1.0)
    }

    /// Calculate current slot index (0-335).
    fn current_slot_index() -> usize {
        let now = Local::now();
        Self::slot_index_for_time(
            now.weekday().num_days_from_monday() as usize,
            now.hour() as usize,
            now.minute() as usize,
        )
    }

    /// Calculate slot index for a specific time.
    fn slot_index_for_time(day_of_week: usize, hour: usize, minute: usize) -> usize {
        let half_hour_slot = if minute >= 30 { 1 } else { 0 };
        (day_of_week * 48) + (hour * 2) + half_hour_slot
    }
}

impl Default for UsageProfile {
    fn default() -> Self {
        // alpha=0.1: moderate adaptation
        // default_power=10.0: reasonable initial guess
        Self::new(0.1, 10.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slot_index_calculation() {
        // monday 00:00 -> slot 0
        assert_eq!(UsageProfile::slot_index_for_time(0, 0, 0), 0);

        // monday 00:30 -> slot 1
        assert_eq!(UsageProfile::slot_index_for_time(0, 0, 30), 1);

        // monday 01:00 -> slot 2
        assert_eq!(UsageProfile::slot_index_for_time(0, 1, 0), 2);

        // monday 23:30 -> slot 47
        assert_eq!(UsageProfile::slot_index_for_time(0, 23, 30), 47);

        // tuesday 00:00 -> slot 48
        assert_eq!(UsageProfile::slot_index_for_time(1, 0, 0), 48);

        // sunday 23:30 -> slot 335 (last slot)
        assert_eq!(UsageProfile::slot_index_for_time(6, 23, 30), 335);
    }

    #[test]
    fn test_profile_update() {
        let mut profile = UsageProfile::new(0.5, 10.0);

        let slot_idx = UsageProfile::current_slot_index();
        let initial = profile.slots[slot_idx];

        // update with 20W
        profile.update(20.0);

        // should be between initial and new value
        let updated = profile.slots[slot_idx];
        assert!(updated > initial);
        assert!(updated < 20.0);
        assert_eq!(profile.counts[slot_idx], 1);
    }

    #[test]
    fn test_profile_convergence() {
        let mut profile = UsageProfile::new(0.2, 10.0);

        let slot_idx = UsageProfile::current_slot_index();

        // repeatedly update with 25W
        for _ in 0..50 {
            profile.update(25.0);
        }

        let converged = profile.slots[slot_idx];
        // should converge close to 25W
        assert!((converged - 25.0).abs() < 1.0);
    }

    #[test]
    fn test_confidence_increases() {
        let mut profile = UsageProfile::default();

        assert!(profile.get_confidence() < 0.1);

        // add 25 samples
        for _ in 0..25 {
            profile.update(15.0);
        }

        let confidence = profile.get_confidence();
        assert!((0.49..=0.51).contains(&confidence));

        // add 25 more samples (total 50)
        for _ in 0..25 {
            profile.update(15.0);
        }

        let confidence = profile.get_confidence();
        assert!(confidence >= 0.99);
    }

    #[test]
    fn test_get_power_at() {
        let profile = UsageProfile::new(0.1, 12.0);

        // should return a value from the slots array
        let power_1h = profile.get_power_at(3600);
        assert!(power_1h > 0.0);

        let power_12h = profile.get_power_at(12 * 3600);
        assert!(power_12h > 0.0);
    }
}
