//! Stores a user's historical power usage. This data is used to make informed
//! predictions on future battery drain and estimated time remaining.

use std::{fs, path::PathBuf, time::Duration};

use anyhow::{Context, Result};
use chrono::{DateTime, Datelike, Local, TimeDelta, Timelike};
use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

use crate::battery::{ChargingStatus, STATISTICS_ALPHA, sysfs::SysfsReading};

/// Number of Fourier harmonics used to model the weekly power-usage cycle.
///
/// 42 harmonics resolve variations down to a 4-hour period (168 h / 42).
const HARMONICS: usize = 42;

/// Duration of one full model period: one week in seconds.
const PERIOD_SECS: f64 = 7.0 * 24.0 * 3600.0;

/// Determines how much new power readings affect historial averages.
///
/// Maintains about a month of readings per slot.
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
        if now.signed_duration_since(self.last_save) >= TimeDelta::minutes(5) {
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

        // update Fourier coefficients via online EMA:
        //   aₖ ← (1 - α) · aₖ + 2α · P · cos(ωₖ t)
        //   bₖ ← (1 - α) · bₖ + 2α · P · sin(ωₖ t)
        let t = week_offset_secs(now);
        for k in 1..=HARMONICS {
            let angle = 2.0 * std::f64::consts::PI * k as f64 / PERIOD_SECS * t;
            self.cosine_coeffs[k - 1] = self.cosine_coeffs[k - 1] * (1.0 - LEARNING_RATE)
                + 2.0 * power_now * angle.cos() * LEARNING_RATE;
            self.sine_coeffs[k - 1] = self.sine_coeffs[k - 1] * (1.0 - LEARNING_RATE)
                + 2.0 * power_now * angle.sin() * LEARNING_RATE;
        }
    }

    /// Returns the modeled power draw, in watts, at the given time.
    ///
    /// Evaluates the truncated Fourier series
    /// `P(t) = a₀ + Σₖ [aₖ cos(ωₖ t) + bₖ sin(ωₖ t)]`
    /// and clamps the result to zero to prevent negative power predictions.
    pub fn predict_discharging_power_at(&self, when: DateTime<Local>) -> f64 {
        let t = week_offset_secs(when);
        let mut power = self.ema_power;

        for k in 1..=HARMONICS {
            let angle = 2.0 * std::f64::consts::PI * k as f64 / PERIOD_SECS * t;
            power +=
                self.cosine_coeffs[k - 1] * angle.cos() + self.sine_coeffs[k - 1] * angle.sin();
        }

        // clamp to non-negative: a fitted curve can dip below zero
        power.max(0.0)
    }

    /// Computes the exact integral of modeled power from week offset `t1` to
    /// `t1 + delta_secs`, returning energy in watt-seconds.
    ///
    /// Uses the closed-form antiderivative of each sinusoidal term:
    /// `∫ aₖ cos(ω t) dt = (aₖ/ω) sin(ω t)`,
    /// `∫ bₖ sin(ω t) dt = -(bₖ/ω) cos(ω t)`.
    fn energy_integral(&self, t1: f64, delta_secs: f64) -> f64 {
        let t2 = t1 + delta_secs;
        let mut energy = self.ema_power * delta_secs;

        for k in 1..=HARMONICS {
            let omega = 2.0 * std::f64::consts::PI * k as f64 / PERIOD_SECS;
            let inv_omega = 1.0 / omega;
            energy +=
                self.cosine_coeffs[k - 1] * inv_omega * ((omega * t2).sin() - (omega * t1).sin());
            energy -=
                self.sine_coeffs[k - 1] * inv_omega * ((omega * t2).cos() - (omega * t1).cos());
        }

        energy
    }

    /// Uses integration over stored historical time-slot data to determine how
    /// long it will take for the battery to deplete entirely.
    ///
    /// Steps forward through 15-minute slots starting from `from_when`,
    /// subtracting the predicted power draw each slot until `wh_remaining`
    /// reaches zero.
    pub fn predict_time_to_empty(
        &mut self,
        from_when: DateTime<Local>,
        mut wh_remaining: f64,
    ) -> Duration {
        if wh_remaining == 0.0 {
            return Duration::ZERO;
        }

        // 15-minute slots, 672 per week (4 slots/hour × 24 h × 7 days)
        let hours_per_slot = 0.25_f64;
        let mut elapsed = Duration::ZERO;

        // step forward slot by slot until energy runs out or a week has passed
        // (guard against infinite loops when history is zero everywhere)
        for _ in 0..672_u32 {
            let power_watts = self.predict_discharging_power_at(from_when + elapsed);

            let energy_this_slot = power_watts * hours_per_slot;
            if energy_this_slot >= wh_remaining {
                // battery drains partway through this slot — interpolate the
                // fraction of the slot consumed
                let fraction = wh_remaining / energy_this_slot;
                elapsed += Duration::from_secs_f64(900.0 * fraction);
                break;
            }

            wh_remaining -= energy_this_slot;
            elapsed += Duration::from_mins(15);
        }

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

    /// Whether `ema` has been seeded with at least one value.
    initialized: bool,
}

impl DischargingStatistics {
    fn update(&mut self, new_time_to_empty_timestamp: i64) {
        let value = new_time_to_empty_timestamp as f64;

        if !self.initialized {
            self.ema = value;
            self.initialized = true;
            // variance is undefined for the first sample; skip logging it
        } else {
            self.ema = self.ema * (1.0 - STATISTICS_ALPHA) + value * STATISTICS_ALPHA;
            let diff = value - self.ema;
            self.variance_ema =
                self.variance_ema * (1.0 - STATISTICS_ALPHA) + (diff * diff) * STATISTICS_ALPHA;
        }

        let standard_deviation = self.variance_ema.sqrt();

        log::debug!("-----discharging statistics--------------------");
        if let Some(new_utc) = DateTime::from_timestamp(new_time_to_empty_timestamp, 0) {
            log::debug!(
                " time-to-empty estimate now: {}",
                DateTime::<Local>::from(new_utc)
            );
        }
        if let Some(ema_utc) = DateTime::from_timestamp(self.ema as i64, 0) {
            log::debug!(
                "time-to-empty estimate ema: {}",
                DateTime::<Local>::from(ema_utc)
            );
        }

        log::debug!(
            "                   variance: {:>12.1} sec^2",
            self.variance_ema
        );
        log::debug!(
            "                          σ: {:>12.1} sec",
            standard_deviation
        );

        if self.variance_ema > 0.0 {
            log::debug!("- - - - - - - - - - - - - - - - - - - - - ");
            log::debug!(
                "current estimate is {:.1} σ from ema estimate",
                (value - self.ema).abs() / standard_deviation
            );
        }
    }
}
