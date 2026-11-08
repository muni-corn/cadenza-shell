use std::time::{Duration, Instant};

/// How quickly a short-term average adjusts to load changes.
const SHORT_TAU: Duration = Duration::from_mins(3);

/// How quickly a long-term average adjusts to load changes.
const LONG_TAU: Duration = Duration::from_hours(2);

/// Threshold for which estimates longer than this are shown as "a while".
const MAX_TIME_TO_EMPTY: Duration = Duration::from_hours(72 * 3);

/// Threshold for which the battery is effectively idle and any estimate would
/// be meaningless.
const MIN_POWER_WATTS: f64 = 0.05;

/// Learns the discharge power draw from readings and estimates how long the
/// remaining charge will last.
///
/// Keeps two exponential moving averages of power: a short one (minutes) that
/// tracks the current load and a long one (hours) that remembers the typical
/// load. Updates are weighted by the time since the previous reading, so
/// irregularly spaced readings (udev events arrive whenever the battery
/// changes) don't skew the averages.
///
/// # Example
/// ```ignore
/// let mut model = PowerModel::default();
/// model.update(10.0, Instant::now());
/// assert_eq!(model.time_to_empty(10.0), Duration::from_secs(3600));
/// ```
#[derive(Debug, Default)]
pub struct PowerModel {
    averages: Option<Averages>,
}

#[derive(Debug)]
struct Averages {
    short_watts: f64,
    long_watts: f64,
    last_update: Instant,
}

impl PowerModel {
    /// Records a power reading taken while discharging.
    ///
    /// `now` is provided as a parameter for testability.
    pub fn update(&mut self, watts: f64, now: Instant) {
        let Some(averages) = &mut self.averages else {
            self.averages = Some(Averages {
                short_watts: watts,
                long_watts: watts,
                last_update: now,
            });
            return;
        };

        let elapsed = now.saturating_duration_since(averages.last_update);
        averages.short_watts += smoothing(elapsed, SHORT_TAU) * (watts - averages.short_watts);
        averages.long_watts += smoothing(elapsed, LONG_TAU) * (watts - averages.long_watts);
        averages.last_update = now;
    }

    /// Estimates how long `remaining_wh` will last at the learned rates of
    /// discharge.
    ///
    /// A more full battery favors the long-range rate for estimates, while a
    /// battery near empty will favor the short-range rate.
    ///
    /// Returns the maximum estimate until a discharge reading has been taken.
    pub fn time_to_empty(&self, remaining_wh: f64, capacity_wh: f64) -> Duration {
        let Some(averages) = &self.averages else {
            return MAX_TIME_TO_EMPTY;
        };

        let long_weight = remaining_wh / capacity_wh;
        let short_weight = 1.0 - long_weight;
        let watts = short_weight * averages.short_watts + long_weight * averages.long_watts;
        if watts < MIN_POWER_WATTS {
            return MAX_TIME_TO_EMPTY;
        }

        Duration::from_secs_f64((remaining_wh.max(0.0) / watts) * 3600.0).min(MAX_TIME_TO_EMPTY)
    }
}

/// The fraction of the way a moving average with time constant `tau` moves
/// toward a new reading after `elapsed`.
fn smoothing(elapsed: Duration, tau: Duration) -> f64 {
    1.0 - (-elapsed.as_secs_f64() / tau.as_secs_f64()).exp()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minutes(minutes: u64) -> Duration {
        Duration::from_secs(minutes * 60)
    }

    #[test]
    fn first_reading_sets_the_estimate() {
        let mut model = PowerModel::default();
        model.update(10.0, Instant::now());

        assert_eq!(model.time_to_empty(10.0, 100.0), Duration::from_secs(3600));
    }

    #[test]
    fn unknown_draw_gives_the_maximum_estimate() {
        assert_eq!(
            PowerModel::default().time_to_empty(50.0, 100.0),
            MAX_TIME_TO_EMPTY
        );
    }

    #[test]
    fn idle_draw_gives_the_maximum_estimate() {
        let mut model = PowerModel::default();
        model.update(0.0, Instant::now());

        assert_eq!(model.time_to_empty(50.0, 100.0), MAX_TIME_TO_EMPTY);
    }

    #[test]
    fn reacts_to_a_heavier_load_within_minutes() {
        let start = Instant::now();
        let mut model = PowerModel::default();
        model.update(5.0, start);

        // ten minutes at four times the load, sampled every ten seconds
        for step in 1..=60 {
            model.update(20.0, start + Duration::from_secs(step * 10));
        }

        // the estimate should have moved most of the way from 10 h to 2.5 h
        let estimate = model.time_to_empty(50.0, 100.0);
        assert!(estimate < Duration::from_secs(4 * 3600), "{estimate:?}");
    }

    #[test]
    fn sample_spacing_does_not_change_the_result() {
        let start = Instant::now();

        let mut sparse = PowerModel::default();
        sparse.update(5.0, start);
        sparse.update(20.0, start + minutes(10));

        let mut dense = PowerModel::default();
        dense.update(5.0, start);
        for step in 1..=600 {
            dense.update(20.0, start + Duration::from_secs(step));
        }

        let difference = sparse
            .time_to_empty(50.0, 100.0)
            .abs_diff(dense.time_to_empty(50.0, 100.0));
        assert!(difference < Duration::from_secs(60), "{difference:?}");
    }

    #[test]
    fn negative_remaining_energy_is_zero() {
        let mut model = PowerModel::default();
        model.update(10.0, Instant::now());

        assert_eq!(model.time_to_empty(-1.0, 100.0), Duration::ZERO);
    }
}
