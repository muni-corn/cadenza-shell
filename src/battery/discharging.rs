//! Stores a user's historical power usage. This data is used to make informed
//! predictions on future battery drain and estimated time remaining.

use std::{fs, path::PathBuf, time::Duration};

use anyhow::{Context, Result};
use chrono::{DateTime, Datelike, Local, Timelike};
use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

use crate::battery::{ChargingStatus, SAVE_INTERVAL, STATISTICS_ALPHA, sysfs::SysfsReading};

/// Number of Fourier harmonics used to model the weekly power-usage cycle.
///
/// 28 harmonics resolve variations down to a 6-hour period (168 h / 28),
/// which is enough to capture daily and half-day usage patterns while keeping
/// the serialized state small. Any existing state with a different array size
/// will fail to deserialize and fall back to a fresh profile.
const HARMONICS: usize = 28;

/// Duration of one full model period: one week in seconds.
const PERIOD_SECS: f64 = 7.0 * 24.0 * 3600.0;

/// Maximum time-to-empty prediction. Estimates beyond this are capped and the
/// battery tile already displays "Until someday" for durations this long.
const MAX_TTE: Duration = Duration::from_secs(48 * 3_600);

/// Determines how much new power readings affect historical averages.
///
/// Maintains roughly a month of readings before older observations decay.
const LEARNING_RATE: f64 = 1. / 360.;

#[derive(Deserialize, Serialize)]
pub struct DischargeProfile {
    /// Exponential moving average of instantaneous power draw, in watts.
    ema_power: f64,

    /// Fourier cosine coefficients for the weekly power-usage cycle.
    #[serde(with = "BigArray")]
    cosine_coeffs: [f64; HARMONICS],

    /// Fourier sine coefficients for the weekly power-usage cycle.
    #[serde(with = "BigArray")]
    sine_coeffs: [f64; HARMONICS],

    /// The last time history was persisted to disk.
    #[serde(skip)]
    last_save: DateTime<Local>,

    /// Current statistics on battery discharge.
    #[serde(skip)]
    discharging_statistics: DischargingStatistics,
}

impl Default for DischargeProfile {
    fn default() -> Self {
        Self {
            ema_power: Default::default(),
            cosine_coeffs: [0.0; HARMONICS],
            sine_coeffs: [0.0; HARMONICS],
            last_save: Local::now(),
            discharging_statistics: Default::default(),
        }
    }
}

impl DischargeProfile {
    /// Updates historical records based on a current reading of the device's
    /// power state.
    pub fn update(&mut self, reading: &SysfsReading) {
        let power_now = reading.power_watts();

        match reading.status {
            ChargingStatus::Discharging => {
                self.update_discharging(power_now);
            }

            // do nothing otherwise
            _ => {
                log::warn!("update called in DischargeProfile while not discharging");
                return;
            }
        }

        // save state if 5 minutes or more have passed
        let now = Local::now();
        if now.signed_duration_since(self.last_save) >= SAVE_INTERVAL {
            if let Err(e) = self.save_to_disk() {
                log::error!("couldn't save discharge profile: {e}");
            } else {
                self.last_save = now;
            }
        }
    }

    fn update_discharging(&mut self, power_now: f64) {
        let now = Local::now();

        // seed the EMA on first observation; otherwise apply moving average
        if self.ema_power == 0.0 {
            self.ema_power = power_now;
        } else {
            self.ema_power = self.ema_power * (1.0 - LEARNING_RATE) + power_now * LEARNING_RATE;
        }

        // update Fourier coefficients via online EMA using the mean-subtracted
        // deviation signal. projecting the raw power_now onto each harmonic would
        // introduce DC leakage: coefficients pick up the level of ema_power and
        // constructively interfere at prediction time, producing over-predictions.
        // subtracting the mean before projection keeps coefficients zero-mean and
        // unbiased regardless of the sampling distribution.
        //
        //   aₖ ← (1 - α) · aₖ + 2α · (P − P̄) · cos(ωₖ t)
        //   bₖ ← (1 - α) · bₖ + 2α · (P − P̄) · sin(ωₖ t)
        let deviation = power_now - self.ema_power;
        let t = week_offset_secs(now);
        for k in 1..=HARMONICS {
            let angle = 2.0 * std::f64::consts::PI * k as f64 / PERIOD_SECS * t;
            self.cosine_coeffs[k - 1] = self.cosine_coeffs[k - 1] * (1.0 - LEARNING_RATE)
                + 2.0 * deviation * angle.cos() * LEARNING_RATE;
            self.sine_coeffs[k - 1] = self.sine_coeffs[k - 1] * (1.0 - LEARNING_RATE)
                + 2.0 * deviation * angle.sin() * LEARNING_RATE;
        }
    }

    /// Returns the modeled power draw, in watts, at the given time.
    ///
    /// Evaluates the truncated Fourier series
    /// `P(t) = a₀ + Σₖ [aₖ cos(ωₖ t) + bₖ sin(ωₖ t)]`
    /// and clamps the result to `[0, 3·ema_power]`. The lower bound prevents
    /// negative power predictions; the upper bound guards against residual
    /// estimation noise producing wild over-predictions while still allowing
    /// genuine high-power spikes.
    pub fn predict_discharging_power_at(&self, when: DateTime<Local>) -> f64 {
        let t = week_offset_secs(when);
        let mut power = self.ema_power;

        for k in 1..=HARMONICS {
            let angle = 2.0 * std::f64::consts::PI * k as f64 / PERIOD_SECS * t;
            power +=
                self.cosine_coeffs[k - 1] * angle.cos() + self.sine_coeffs[k - 1] * angle.sin();
        }

        power.clamp(0.0, 3.0 * self.ema_power.max(f64::MIN_POSITIVE))
    }

    /// Estimates the energy consumed, in watt-seconds, if the device draws
    /// power according to the Fourier model from `from` for `delta_secs`
    /// seconds.
    ///
    /// Uses the midpoint rule with 1-minute steps so that the same clamped
    /// prediction function drives both the integral and its derivative in
    /// Newton's method, keeping them consistent.
    fn energy_integral(&self, from: DateTime<Local>, delta_secs: f64) -> f64 {
        const STEP_SECS: f64 = 60.0;

        let steps = (delta_secs / STEP_SECS).ceil() as u64;
        if steps == 0 {
            return 0.0;
        }

        let actual_step = delta_secs / steps as f64;
        let half_step = Duration::from_secs_f64(actual_step * 0.5);
        let mut energy = 0.0;

        for i in 0..steps {
            let mid = from + Duration::from_secs_f64(i as f64 * actual_step) + half_step;
            energy += self.predict_discharging_power_at(mid) * actual_step;
        }

        energy
    }

    /// Uses numerical energy integration with Newton's method to determine
    /// how long it will take for the battery to deplete entirely.
    ///
    /// Capped at [`MAX_TTE`] (48 hours).
    pub fn predict_time_to_empty(
        &mut self,
        from_when: DateTime<Local>,
        wh_remaining: f64,
    ) -> Duration {
        if wh_remaining == 0.0 {
            return Duration::ZERO;
        }

        if self.ema_power == 0.0 {
            return MAX_TTE;
        }

        let ws_remaining = wh_remaining * 3_600.0;
        let max_secs = MAX_TTE.as_secs_f64();

        // initial guess: linear estimate from overall EMA power draw
        let mut delta = (ws_remaining / self.ema_power).min(max_secs);

        // bisection bounds: lo is always under-estimate, hi is always over-estimate.
        // start with the tightest bracket we can establish cheaply.
        let mut lo = 0.0_f64;
        let mut hi = max_secs;

        // hybrid Newton / bisection: find Δt such that
        //   f(Δt) = energy_integral(from, Δt) - ws_remaining = 0
        //
        // at each step we prefer the Newton update (fast, quadratic convergence),
        // but fall back to the bisection midpoint when Newton would step outside
        // the known bracket or when f'(Δt) is zero (zero-power night-time region).
        // this prevents the original code from halting prematurely when the
        // predicted power at the endpoint is zero.
        for _ in 0..40 {
            let f = self.energy_integral(from_when, delta) - ws_remaining;

            // tighten the bracket around the root
            if f < 0.0 {
                lo = lo.max(delta);
            } else {
                hi = hi.min(delta);
            }

            if hi - lo < 1.0 {
                // bracket is within 1-second precision
                break;
            }

            let future = from_when + Duration::from_secs_f64(delta);
            let f_prime = self.predict_discharging_power_at(future);

            let next = if f_prime > 0.0 {
                let newton = delta - f / f_prime;
                if newton > lo && newton < hi {
                    newton
                } else {
                    // Newton stepped outside the bracket; bisect instead
                    (lo + hi) * 0.5
                }
            } else {
                // zero-power region at the endpoint: bisect toward lo
                (lo + hi) * 0.5
            };

            if (next - delta).abs() < 1.0 {
                break;
            }

            delta = next;
        }

        let elapsed = Duration::from_secs_f64(delta);

        self.discharging_statistics
            .update((from_when + elapsed).timestamp());

        elapsed
    }

    /// Get the path to the history file.
    fn get_state_path() -> Result<PathBuf> {
        Ok(get_state_directory()?.join("discharge_profile.json"))
    }

    pub fn read_from_disk() -> Result<Self> {
        let path = Self::get_state_path()?;
        let json = fs::read_to_string(&path).context("couldn't read power history")?;

        Ok(serde_json::from_str(&json)?)
    }

    fn save_to_disk(&self) -> Result<()> {
        let json = serde_json::to_string_pretty(&self)?;
        let path = Self::get_state_path()?;
        fs::write(&path, json).context("couldn't write predictor state")?;
        log::debug!("saved power history state to {:?}", path);
        Ok(())
    }
}

pub(super) fn get_state_directory() -> Result<PathBuf> {
    let state_dir = dirs::state_dir()
        .or_else(dirs::data_local_dir)
        .context("couldn't find state directory")?;
    let cadenza_state = state_dir.join("cadenza-shell");
    fs::create_dir_all(&cadenza_state).context("couldn't create state directory")?;
    Ok(cadenza_state)
}

/// Returns the number of seconds elapsed since Monday 00:00:00 local time in
/// the current week.
fn week_offset_secs(when: DateTime<Local>) -> f64 {
    let day_of_week = when.weekday().num_days_from_monday() as f64;
    let hour = when.hour() as f64;
    let minute = when.minute() as f64;
    let second = when.second() as f64;

    day_of_week * 86_400.0 + hour * 3_600.0 + minute * 60.0 + second
}

#[derive(Default)]
struct DischargingStatistics {
    /// Exponential moving average of predicted time-to-empty timestamps
    /// (seconds).
    ema: f64,

    /// EMA of the squared deviation from `ema`, used as a variance estimate.
    variance_ema: f64,

    /// EMA of the signed deviation from `ema`.
    ///
    /// Positive means recent estimates are running later (more optimistic) than
    /// the long-run average; negative means they are running earlier
    /// (more pessimistic).
    deviation_ema: f64,

    /// Total number of time-to-empty estimates recorded.
    n_updates: u64,

    /// Smallest (most pessimistic) predicted end-of-charge timestamp observed.
    min_observed: Option<i64>,

    /// Largest (most optimistic) predicted end-of-charge timestamp observed.
    max_observed: Option<i64>,

    /// Whether `ema` has been seeded with at least one value.
    initialized: bool,
}

impl DischargingStatistics {
    fn update(&mut self, new_time_to_empty_timestamp: i64) {
        let value = new_time_to_empty_timestamp as f64;
        self.n_updates += 1;

        self.min_observed = Some(match self.min_observed {
            Some(prev) => prev.min(new_time_to_empty_timestamp),
            _ => new_time_to_empty_timestamp,
        });
        self.max_observed = Some(match self.max_observed {
            Some(prev) => prev.max(new_time_to_empty_timestamp),
            _ => new_time_to_empty_timestamp,
        });

        if !self.initialized {
            self.ema = value;
            self.initialized = true;
            // variance is undefined for the first sample; skip logging it
        } else {
            self.ema = self.ema * (1.0 - STATISTICS_ALPHA) + value * STATISTICS_ALPHA;
            let diff = value - self.ema;
            self.variance_ema =
                self.variance_ema * (1.0 - STATISTICS_ALPHA) + (diff * diff) * STATISTICS_ALPHA;
            self.deviation_ema =
                self.deviation_ema * (1.0 - STATISTICS_ALPHA) + diff * STATISTICS_ALPHA;
        }

        let standard_deviation = self.variance_ema.sqrt();

        log::debug!("-----discharging statistics--------------------");

        if let Some(new_utc) = DateTime::from_timestamp(new_time_to_empty_timestamp, 0) {
            log::debug!(
                "{:>17}: {}",
                "tte estimate now",
                DateTime::<Local>::from(new_utc)
            );
        }
        if let Some(ema_utc) = DateTime::from_timestamp(self.ema as i64, 0) {
            log::debug!(
                "{:>17}: {}",
                "tte estimate ema",
                DateTime::<Local>::from(ema_utc)
            );
        }

        log::debug!("- - - - - - - - - - - - - - - - - - - - - ");

        if let Some(min_ts) = self.min_observed
            && let Some(min_utc) = DateTime::from_timestamp(min_ts, 0)
        {
            log::debug!(
                "{:>17}: {}",
                "earliest end",
                DateTime::<Local>::from(min_utc)
            );
        }

        let pessimistic_ts = self.ema - standard_deviation;
        if let Some(pessimistic_utc) = DateTime::from_timestamp(pessimistic_ts as i64, 0) {
            log::debug!(
                "{:>17}: {}",
                "pessimistic end",
                DateTime::<Local>::from(pessimistic_utc)
            );
        }

        let pessimistic_ts = self.ema + standard_deviation;
        if let Some(optimistic_utc) = DateTime::from_timestamp(pessimistic_ts as i64, 0) {
            log::debug!(
                "{:>17}: {}",
                "optimistic end",
                DateTime::<Local>::from(optimistic_utc)
            );
        }

        if let Some(max_ts) = self.max_observed
            && let Some(max_utc) = DateTime::from_timestamp(max_ts, 0)
        {
            log::debug!("{:>17}: {}", "latest end", DateTime::<Local>::from(max_utc));
        }

        log::debug!("- - - - - - - - - - - - - - - - - - - - - ");

        log::debug!("{:>17}: {:>6}", "predictions made", self.n_updates);

        if let (Some(min_ts), Some(max_ts)) = (self.min_observed, self.max_observed) {
            let span_hours = (max_ts - min_ts) as f64 / 3600.;
            log::debug!("{:>17}: {:>6.1} h", "observed tte span", span_hours);
        }

        log::debug!(
            "{:>17}: {:>6.1} min^2",
            "variance",
            self.variance_ema / 3600.
        );

        log::debug!("{:>17}: {:>6.1} min", "σ", standard_deviation / 60.);

        log::debug!(
            "{:>17}: {:>+6.1} min",
            "bias (ema Δ)",
            self.deviation_ema / 60.0
        );

        let bias_now = value - self.ema;
        if self.variance_ema > 0.0 {
            log::debug!("- - - - - - - - - - - - - - - - - - - - - ");
            log::debug!(
                "current estimate is {:+.1} min ({:.1}σ) from ema estimate",
                bias_now / 60.,
                bias_now.abs() / standard_deviation
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::Local;

    use super::*;

    // ── helpers ───────────────────────────────────────────────────────────────

    /// Return a [`DischargeProfile`] whose EMA power is `power_watts` and
    /// whose Fourier coefficients are all zero (i.e., constant-power model).
    fn constant_power_profile(power_watts: f64) -> DischargeProfile {
        DischargeProfile {
            ema_power: power_watts,
            ..Default::default()
        }
    }

    /// Drive `profile` with `n` observations of `power_watts` sampled at 15-
    /// minute intervals starting from `base`, and return the trained profile.
    fn train_constant(
        mut profile: DischargeProfile,
        base: DateTime<Local>,
        power_watts: f64,
        n: usize,
    ) -> DischargeProfile {
        for i in 0..n {
            let when = base + Duration::from_secs(i as u64 * 15 * 60);
            // bypass the ChargingStatus check and call the inner fn directly
            let t = week_offset_secs(when);
            let power_now = power_watts;

            if profile.ema_power == 0.0 {
                profile.ema_power = power_now;
            } else {
                profile.ema_power =
                    profile.ema_power * (1.0 - LEARNING_RATE) + power_now * LEARNING_RATE;
            }

            let deviation = power_now - profile.ema_power;
            for k in 1..=HARMONICS {
                let angle = 2.0 * std::f64::consts::PI * k as f64 / PERIOD_SECS * t;
                profile.cosine_coeffs[k - 1] = profile.cosine_coeffs[k - 1] * (1.0 - LEARNING_RATE)
                    + 2.0 * deviation * angle.cos() * LEARNING_RATE;
                profile.sine_coeffs[k - 1] = profile.sine_coeffs[k - 1] * (1.0 - LEARNING_RATE)
                    + 2.0 * deviation * angle.sin() * LEARNING_RATE;
            }
        }
        profile
    }

    // ── predict_discharging_power_at ──────────────────────────────────────────

    #[test]
    fn predict_power_zero_coefficients_returns_ema() {
        // when all Fourier coefficients are zero the series collapses to a₀ = ema_power
        let power = 12.5;
        let profile = constant_power_profile(power);
        let when = Local::now();
        let predicted = profile.predict_discharging_power_at(when);
        assert!(
            (predicted - power).abs() < 1e-9,
            "expected {power} W, got {predicted} W"
        );
    }

    #[test]
    fn predict_power_clamped_to_zero_minimum() {
        // even with wildly negative coefficients the prediction must be non-negative
        let mut profile = constant_power_profile(1.0);
        // force a large negative perturbation into the first coefficient
        profile.cosine_coeffs[0] = -1000.0;
        let predicted = profile.predict_discharging_power_at(Local::now());
        assert!(predicted >= 0.0, "prediction was negative: {predicted}");
    }

    #[test]
    fn predict_power_clamped_to_3x_ema_maximum() {
        let ema = 10.0;
        let mut profile = constant_power_profile(ema);
        // force a massive positive perturbation
        profile.cosine_coeffs[0] = 1_000.0;
        let predicted = profile.predict_discharging_power_at(Local::now());
        assert!(
            predicted <= 3.0 * ema + 1e-9,
            "prediction exceeded 3x ema: {predicted}"
        );
    }

    // ── energy_integral ───────────────────────────────────────────────────────

    #[test]
    fn energy_integral_constant_power_matches_analytic() {
        // for zero coefficients, integral = ema_power * delta_secs
        let power = 8.0;
        let profile = constant_power_profile(power);
        let from = Local::now();
        let delta = 3_600.0; // 1 hour
        let energy = profile.energy_integral(from, delta);
        let expected = power * delta;
        // allow up to 0.1% error from midpoint-rule approximation
        assert!(
            (energy - expected).abs() / expected < 0.001,
            "integral {energy:.1} Ws expected {expected:.1} Ws"
        );
    }

    #[test]
    fn energy_integral_zero_delta_returns_zero() {
        let profile = constant_power_profile(10.0);
        let energy = profile.energy_integral(Local::now(), 0.0);
        assert_eq!(energy, 0.0);
    }

    // ── predict_time_to_empty ─────────────────────────────────────────────────

    #[test]
    fn tte_zero_wh_returns_zero_duration() {
        let mut profile = constant_power_profile(10.0);
        let tte = profile.predict_time_to_empty(Local::now(), 0.0);
        assert_eq!(tte, Duration::ZERO);
    }

    #[test]
    fn tte_zero_ema_returns_max_tte() {
        let mut profile = constant_power_profile(0.0);
        let tte = profile.predict_time_to_empty(Local::now(), 30.0);
        assert_eq!(tte, MAX_TTE);
    }

    #[test]
    fn tte_constant_power_matches_analytic() {
        // with zero Fourier coefficients TTE = wh_remaining / ema_power
        let power_w = 12.0;
        let wh = 30.0;
        let mut profile = constant_power_profile(power_w);
        let from = Local::now();
        let tte = profile.predict_time_to_empty(from, wh);

        let expected_secs = wh / power_w * 3_600.0;
        let error_secs = (tte.as_secs_f64() - expected_secs).abs();
        assert!(
            error_secs < 5.0,
            "TTE error {error_secs:.1}s exceeds 5 s (expected {expected_secs:.1}s, got {:.1}s)",
            tte.as_secs_f64()
        );
    }

    #[test]
    fn tte_capped_at_max_tte() {
        // a very large battery and tiny power draw should cap at MAX_TTE
        let mut profile = constant_power_profile(0.1); // 0.1 W
        let tte = profile.predict_time_to_empty(Local::now(), 100.0); // 100 Wh → 1000 h
        assert_eq!(tte, MAX_TTE, "TTE should be capped at MAX_TTE (48 h)");
    }

    #[test]
    fn tte_converges_after_constant_training() {
        // after many constant-power observations the harmonic corrections are
        // close to zero and TTE should still match wh / P within a few percent
        let power_w = 10.0;
        let wh = 20.0;
        let base = Local::now();
        let profile = train_constant(constant_power_profile(power_w), base, power_w, 500);
        let mut profile = profile;
        let tte = profile.predict_time_to_empty(base, wh);

        let expected_secs = wh / power_w * 3_600.0;
        let relative_error = (tte.as_secs_f64() - expected_secs).abs() / expected_secs;
        assert!(
            relative_error < 0.05,
            "relative TTE error {:.1}% exceeds 5% after training",
            relative_error * 100.0
        );
    }
}
