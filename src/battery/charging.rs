//! CC/CV charging phase detection and time-to-full prediction.
//!
//! Learned per-device charging parameters ([`ChargeProfile`]) are persisted to
//! disk and updated at the end of each charging session. The active session is
//! tracked in-memory by [`ChargingSession`].

use std::{fs, path::PathBuf, time::Duration};

use anyhow::{Context, Result};
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

use super::history::get_state_directory;

/// Learning rate for exponential moving averages applied to [`ChargeProfile`]
/// fields after each completed charging session.
const PROFILE_LEARNING_RATE: f64 = 0.2;

/// Learned CC/CV charging parameters for this device.
///
/// All fields are updated via exponential moving average after each charging
/// session that contains a detectable CC-to-CV transition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChargeProfile {
    /// Average current during the CC (constant-current) phase, in microamperes.
    pub cc_current_ua: f64,

    /// Battery state-of-charge fraction at which the CC-to-CV transition is
    /// typically observed, in the range `[0, 1]`.
    pub switch_percentage: f64,

    /// Exponential decay time constant for the CV phase, in seconds.
    /// Models `I(t) = I₀ · exp(−t / tau)`.
    pub cv_tau_secs: f64,

    /// Current at the moment the CC-to-CV transition is detected, in
    /// microamperes.
    pub cv_start_current_ua: f64,

    /// Number of completed charging sessions that contributed to this profile.
    pub sessions_learned: u32,
}

impl Default for ChargeProfile {
    fn default() -> Self {
        Self {
            cc_current_ua: 0.0,
            switch_percentage: 0.0,
            cv_tau_secs: 0.0,
            cv_start_current_ua: 0.0,
            sessions_learned: 0,
        }
    }
}

impl ChargeProfile {
    /// Returns `true` if this profile has enough data to make predictions.
    pub fn is_ready(&self) -> bool {
        self.sessions_learned > 0
            && self.cv_tau_secs > 0.0
            && self.cv_start_current_ua > 0.0
            && self.switch_percentage > 0.0
    }

    /// Update the profile with parameters learned from a completed charging
    /// session using an exponential moving average.
    ///
    /// - `cc_current_ua` – plateau current observed during the CC phase (µA).
    /// - `switch_pct` – state-of-charge fraction at which the CC→CV transition
    ///   was detected.
    /// - `cv_start_ua` – current at the moment of the CC→CV transition (µA).
    /// - `tau_secs` – fitted exponential decay constant for the CV phase (s).
    pub fn update(&mut self, cc_current_ua: f64, switch_pct: f64, cv_start_ua: f64, tau_secs: f64) {
        let alpha = PROFILE_LEARNING_RATE;
        let one_minus = 1.0 - alpha;

        if self.sessions_learned == 0 {
            // first session: seed directly rather than blending with zeros
            self.cc_current_ua = cc_current_ua;
            self.switch_percentage = switch_pct;
            self.cv_start_current_ua = cv_start_ua;
            self.cv_tau_secs = tau_secs;
        } else {
            self.cc_current_ua = self.cc_current_ua * one_minus + cc_current_ua * alpha;
            self.switch_percentage = self.switch_percentage * one_minus + switch_pct * alpha;
            self.cv_start_current_ua = self.cv_start_current_ua * one_minus + cv_start_ua * alpha;
            self.cv_tau_secs = self.cv_tau_secs * one_minus + tau_secs * alpha;
        }

        self.sessions_learned += 1;
    }

    // ── persistence ──────────────────────────────────────────────────────────

    fn get_path() -> Result<PathBuf> {
        Ok(get_state_directory()?.join("charge_profile.json"))
    }

    /// Load the [`ChargeProfile`] from disk, returning the default if no file
    /// exists yet.
    pub fn load() -> Self {
        match Self::try_load() {
            Ok(p) => {
                log::info!("loaded charge profile ({} sessions)", p.sessions_learned);
                p
            }
            Err(e) => {
                log::info!("starting fresh charge profile: {e}");
                Self::default()
            }
        }
    }

    fn try_load() -> Result<Self> {
        let path = Self::get_path()?;
        let json = fs::read_to_string(&path).context("reading charge_profile.json")?;
        serde_json::from_str(&json).context("parsing charge_profile.json")
    }

    /// Persist the profile to disk.
    pub fn save(&self) -> Result<()> {
        let path = Self::get_path()?;
        let json = serde_json::to_string_pretty(self)?;
        fs::write(&path, json).context("writing charge_profile.json")?;
        log::debug!("saved charge profile to {path:?}");
        Ok(())
    }
}

// ── active session ───────────────────────────────────────────────────────────

/// The detected phase of a charging session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChargingPhase {
    /// Phase not yet determined; waiting for enough readings.
    #[default]
    Unknown,

    /// Constant-current phase: charger supplies a near-constant current while
    /// voltage rises.
    Cc,

    /// Constant-voltage phase: voltage is held at maximum and current decays
    /// exponentially as the battery approaches full.
    Cv,
}

/// A single timestamped observation recorded during a charging session.
#[derive(Debug, Clone)]
pub struct SessionReading {
    /// Wall-clock time of this reading.
    pub when: DateTime<Local>,

    /// Measured current in microamperes (µA). Always positive (charging).
    pub current_ua: f64,

    /// State of charge as a fraction `[0, 1]`.
    pub percentage: f64,

    /// Remaining capacity in microampere-hours (µAh).
    pub charge_uah: f64,
}

/// Minimum number of CV-phase readings required before the OLS fit is
/// considered valid enough to use for prediction.
const CV_FIT_MIN_READINGS: usize = 3;

/// Number of readings in the rolling window used to compute the median current
/// for phase detection. At 10 s polling this covers ~1 minute.
const PHASE_WINDOW: usize = 6;

/// A current drop to this fraction of the CC plateau triggers a transition
/// check. Chosen to be robust against the noisy dips visible in your data.
const CV_DROP_THRESHOLD: f64 = 0.75;

/// How many consecutive readings below `CV_DROP_THRESHOLD` are required before
/// declaring the CC→CV transition. At 10 s polling this is ~1 minute.
const CV_CONFIRM_READINGS: usize = 6;

// ── CV exponential fitting
// ────────────────────────────────────────────────────

/// Incremental ordinary least-squares fit of `ln(I) = a − t/tau` in log-space,
/// i.e. `I(t) = exp(a) · exp(−t/tau)` where `tau = −1/slope`.
///
/// Accumulates running sums so each new reading is O(1) to incorporate.
#[derive(Debug, Default)]
pub struct CvFit {
    /// Number of data points incorporated so far.
    n: f64,
    /// Σ xᵢ (seconds since CV start)
    sum_x: f64,
    /// Σ yᵢ (ln(current_ua))
    sum_y: f64,
    /// Σ xᵢ²
    sum_xx: f64,
    /// Σ xᵢ yᵢ
    sum_xy: f64,
}

impl CvFit {
    /// Incorporate one new CV-phase data point.
    ///
    /// - `t_secs` – seconds elapsed since the CC→CV transition.
    /// - `current_ua` – charging current at this point (µA). Values ≤ 0 are
    ///   ignored to keep the log transform valid.
    pub fn push(&mut self, t_secs: f64, current_ua: f64) {
        if current_ua <= 0.0 {
            return;
        }
        let x = t_secs;
        let y = current_ua.ln();
        self.n += 1.0;
        self.sum_x += x;
        self.sum_y += y;
        self.sum_xx += x * x;
        self.sum_xy += x * y;
    }

    /// Returns `true` when enough data points have been collected for a
    /// reliable fit.
    pub fn is_ready(&self) -> bool {
        self.n >= CV_FIT_MIN_READINGS as f64
    }

    /// Compute the fitted decay time constant `tau` in seconds.
    ///
    /// Returns `None` if there are insufficient data points or the regression
    /// is degenerate (all readings at the same time).
    pub fn tau_secs(&self) -> Option<f64> {
        if !self.is_ready() {
            return None;
        }
        let denom = self.n * self.sum_xx - self.sum_x * self.sum_x;
        if denom.abs() < f64::EPSILON {
            return None;
        }
        let slope = (self.n * self.sum_xy - self.sum_x * self.sum_y) / denom;
        // slope = -1/tau  =>  tau = -1/slope
        if slope >= 0.0 {
            // non-negative slope means current is not decaying; discard
            return None;
        }
        Some(-1.0 / slope)
    }

    /// Compute the fitted initial current `I₀` (µA) at `t=0` (the transition
    /// point).
    ///
    /// Returns `None` under the same conditions as [`Self::tau_secs`].
    pub fn i0_ua(&self) -> Option<f64> {
        if !self.is_ready() {
            return None;
        }
        let denom = self.n * self.sum_xx - self.sum_x * self.sum_x;
        if denom.abs() < f64::EPSILON {
            return None;
        }
        let intercept = (self.sum_y * self.sum_xx - self.sum_x * self.sum_xy) / denom;
        Some(intercept.exp())
    }
}

// ── active session ─────────────────────────────────────────────────────────

/// Transient state for the charging session that is currently in progress.
///
/// Created when a charger is connected and discarded when it is removed. Phase
/// detection and CV curve fitting operate on the readings accumulated here.
#[derive(Debug, Default)]
pub struct ChargingSession {
    /// All readings collected since the charger was connected.
    pub readings: Vec<SessionReading>,

    /// Currently detected phase.
    pub phase: ChargingPhase,

    /// Smoothed peak current observed while in the CC phase (µA). Updated as
    /// long as `phase == Cc`.
    pub cc_plateau_ua: f64,

    /// Index into `readings` at which the CC→CV transition was detected.
    /// `None` if the transition has not yet been observed.
    pub transition_index: Option<usize>,

    /// Number of consecutive readings that have been below the CV drop
    /// threshold, used to confirm the transition before committing.
    cv_confirm_count: usize,

    /// Incremental OLS fit of the CV exponential decay. Only updated once the
    /// phase is confirmed as CV.
    pub cv_fit: CvFit,
}

impl ChargingSession {
    /// Add a new reading and update the phase detection state.
    ///
    /// `profile` is consulted to provide a learned `switch_percentage` hint
    /// that can accelerate detection. Pass the default profile if none has
    /// been learned yet.
    pub fn push(&mut self, reading: SessionReading, profile: &ChargeProfile) {
        self.readings.push(reading);

        // need at least PHASE_WINDOW readings before making any determination
        if self.readings.len() < PHASE_WINDOW {
            return;
        }

        match self.phase {
            ChargingPhase::Unknown | ChargingPhase::Cc => self.update_cc_phase(profile),
            ChargingPhase::Cv => self.update_cv_fit(),
        }
    }

    fn update_cc_phase(&mut self, profile: &ChargeProfile) {
        let latest = self.readings.last().unwrap(); // safe: checked len above
        let median = self.rolling_median_current();

        // track the peak smoothed current as the CC plateau
        if median > self.cc_plateau_ua {
            self.cc_plateau_ua = median;
        }

        if self.phase == ChargingPhase::Unknown && self.cc_plateau_ua > 0.0 {
            self.phase = ChargingPhase::Cc;
        }

        // if no plateau established yet, nothing more to check
        if self.cc_plateau_ua == 0.0 {
            return;
        }

        // use the learned switch percentage as a gating condition: don't start
        // looking for the transition until we are within 5% of the known point
        let near_switch =
            !profile.is_ready() || latest.percentage >= (profile.switch_percentage - 0.05).max(0.0);

        if !near_switch {
            return;
        }

        // check whether the current has dropped below the CV threshold
        let below_threshold = median < self.cc_plateau_ua * CV_DROP_THRESHOLD;

        if below_threshold {
            self.cv_confirm_count += 1;
        } else {
            // reset confirmation streak on any reading above the threshold
            self.cv_confirm_count = 0;
        }

        if self.cv_confirm_count >= CV_CONFIRM_READINGS {
            // transition confirmed: mark it at the first reading of the streak
            let transition_idx = self.readings.len() - self.cv_confirm_count;
            self.transition_index = Some(transition_idx);
            self.phase = ChargingPhase::Cv;
            log::info!(
                "CC→CV transition detected at index {transition_idx} \
                 (soc={:.1}%, current={:.0} µA)",
                self.readings[transition_idx].percentage * 100.0,
                self.readings[transition_idx].current_ua,
            );

            // seed the CV fit with all readings from the transition onward
            let t0 = self.readings[transition_idx].when;
            for r in &self.readings[transition_idx..] {
                let t_secs = (r.when - t0).num_milliseconds() as f64 / 1000.0;
                self.cv_fit.push(t_secs, r.current_ua);
            }
        }
    }

    /// Incorporate the latest reading into the CV exponential fit.
    fn update_cv_fit(&mut self) {
        let Some(transition_idx) = self.transition_index else {
            return;
        };
        let t0 = self.readings[transition_idx].when;
        let latest = self.readings.last().unwrap(); // safe: we have readings
        let t_secs = (latest.when - t0).num_milliseconds() as f64 / 1000.0;
        self.cv_fit.push(t_secs, latest.current_ua);
    }

    /// Compute the median current (µA) over the most recent [`PHASE_WINDOW`]
    /// readings. Uses a sorted copy, so it is robust to transient spikes and
    /// the load-induced dips visible in real data.
    fn rolling_median_current(&self) -> f64 {
        let window_start = self.readings.len().saturating_sub(PHASE_WINDOW);
        let mut values: Vec<f64> = self.readings[window_start..]
            .iter()
            .map(|r| r.current_ua)
            .collect();
        values.sort_by(f64::total_cmp);
        let mid = values.len() / 2;
        if values.len().is_multiple_of(2) {
            (values[mid - 1] + values[mid]) / 2.0
        } else {
            values[mid]
        }
    }
}

// ── legacy stub (kept until history.rs is fully migrated) ────────────────────

/// Predict the time until the battery is full using the legacy linear-taper
/// coefficient model.
///
/// Returns [`Duration::MAX`] when the battery is already full or no
/// coefficient has been learned yet.
pub fn predict_time_to_full(
    percentage_now: f64,
    wh_capacity: f64,
    charging_coefficient: f64,
) -> Duration {
    if percentage_now >= 1.0 || charging_coefficient == 0.0 {
        return Duration::MAX;
    }
    let estimated_power = charging_coefficient * (1.0 - percentage_now);
    let wh_to_go = wh_capacity * (1.0 - percentage_now);
    let hours_to_full = wh_to_go / estimated_power;
    Duration::from_secs_f64(hours_to_full * 3600.0)
}
