//! CC/CV charging phase detection and time-to-full prediction.
//!
//! Learned per-device charging parameters ([`ChargeProfile`]) are persisted to
//! disk and updated at the end of each charging session. The active session is
//! tracked in-memory by [`ChargingSession`].

use std::{fs, path::PathBuf, time::Duration};

use anyhow::{Context, Result};
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

use super::{history::get_state_directory, sysfs::SysfsReading};

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

// ── session reading
// ───────────────────────────────────────────────────────────

impl SessionReading {
    /// Construct a [`SessionReading`] from a raw sysfs snapshot.
    ///
    /// Returns `None` if the sysfs reading lacks percentage or capacity data.
    pub fn from_sysfs(r: &SysfsReading) -> Option<Self> {
        let percentage = r.percentage()?;
        // prefer µAh; convert from µWh via voltage if necessary
        let charge_uah = r.capacity_now.as_microampere_hours(r.voltage_now) as f64;
        Some(Self {
            when: r.when,
            current_ua: r.current_now.unsigned_abs() as f64,
            percentage,
            charge_uah,
        })
    }
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

    // ── lifecycle ─────────────────────────────────────────────────────────────

    /// Finalise the session and, if a CC→CV transition was observed and the CV
    /// fit is valid, update `profile` with the learned parameters.
    ///
    /// Call this when the charger is disconnected or the status changes away
    /// from `Charging`.
    pub fn end(&self, profile: &mut ChargeProfile) {
        let Some(transition_idx) = self.transition_index else {
            log::debug!("session ended without a detected CC→CV transition; profile unchanged");
            return;
        };

        let Some(tau) = self.cv_fit.tau_secs() else {
            log::debug!("session ended but CV fit is not ready; profile unchanged");
            return;
        };

        let Some(i0) = self.cv_fit.i0_ua() else {
            return;
        };

        let switch_pct = self.readings[transition_idx].percentage;
        let cc_current_ua = self.cc_plateau_ua;

        log::info!(
            "charging session complete: cc={cc_current_ua:.0} µA, \
             switch={:.1}%, tau={tau:.0} s, I₀={i0:.0} µA",
            switch_pct * 100.0,
        );

        profile.update(cc_current_ua, switch_pct, i0, tau);

        if let Err(e) = profile.save() {
            log::error!("couldn't save charge profile: {e}");
        }
    }

    // ── internal helpers ─────────────────────────────────────────────────────

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

// ── prediction ───────────────────────────────────────────────────────────────

/// Predict time to full using the CC/CV model.
///
/// Uses a three-tier strategy:
///
/// 1. **Best** — active session has a confirmed CV phase with a valid OLS fit:
///    integrate the fitted exponential to the `charge_full_uah` target.
/// 2. **Good** — a session is active in CC or early CV, and a learned
///    [`ChargeProfile`] is available: combine a linear CC estimate with the
///    profile's CV time constant.
/// 3. **Fallback** — no useful data yet: simple `charge_remaining / current`
///    linear extrapolation.
///
/// Returns [`Duration::MAX`] when the battery is already full or no current
/// is flowing.
pub fn predict_time_to_full_cc_cv(
    session: &ChargingSession,
    profile: &ChargeProfile,
    current_ua: f64,
    charge_now_uah: f64,
    charge_full_uah: f64,
) -> Duration {
    if charge_now_uah >= charge_full_uah || current_ua <= 0.0 {
        return Duration::MAX;
    }

    let charge_remaining_uah = charge_full_uah - charge_now_uah;

    // tier 1: active CV fit
    if session.phase == ChargingPhase::Cv {
        if let Some(t) = predict_cv_remaining(session, charge_now_uah, charge_full_uah) {
            return t;
        }
    }

    // tier 2: CC phase with a learned profile
    if profile.is_ready()
        && let Some(t) = predict_cc_plus_cv(
            session,
            profile,
            current_ua,
            charge_now_uah,
            charge_full_uah,
        )
    {
        return t;
    }

    // tier 3: linear fallback
    let hours = charge_remaining_uah / current_ua;
    Duration::from_secs_f64(hours * 3600.0)
}

/// Tier 1: integrate the fitted `I(t) = I₀ · exp(−t/tau)` curve from the
/// current time until `∫I dt = charge_remaining_uah`.
///
/// The amount of charge deposited between `t_now` and some future time `T` is:
/// ```text
/// Q = I₀ · tau · (exp(−t_now/tau) − exp(−T/tau))
/// ```
/// Solving for `T − t_now` gives:
/// ```text
/// Δt = −tau · ln(1 − Q / (I(t_now) · tau))
/// ```
fn predict_cv_remaining(
    session: &ChargingSession,
    charge_now_uah: f64,
    charge_full_uah: f64,
) -> Option<Duration> {
    let tau = session.cv_fit.tau_secs()?;
    let i0 = session.cv_fit.i0_ua()?;
    let transition_idx = session.transition_index?;

    // elapsed seconds since the CV transition started
    let t0 = session.readings[transition_idx].when;
    let t_now = session.readings.last()?.when;
    let t_elapsed = (t_now - t0).num_milliseconds() as f64 / 1000.0;

    // current value on the fitted curve at t_now (µA)
    let i_now_fitted = i0 * (-t_elapsed / tau).exp();

    // charge remaining (µAh → µAs for integration, then back)
    let charge_remaining_uas = (charge_full_uah - charge_now_uah) * 3600.0;

    // amount the exponential can ever deliver from t_now: I_now_fitted · tau
    let deliverable = i_now_fitted * tau;
    if charge_remaining_uas >= deliverable {
        // the exponential asymptote falls short of charge_full; don't use
        return None;
    }

    let delta_t_secs = -tau * (1.0 - charge_remaining_uas / deliverable).ln();
    Some(Duration::from_secs_f64(delta_t_secs))
}

/// Tier 2: linear CC estimate to the switch point, then CV estimate using
/// the profile's learned tau.
fn predict_cc_plus_cv(
    session: &ChargingSession,
    profile: &ChargeProfile,
    current_ua: f64,
    charge_now_uah: f64,
    charge_full_uah: f64,
) -> Option<Duration> {
    let switch_uah = profile.switch_percentage * charge_full_uah;

    let cc_secs = if session.phase == ChargingPhase::Cv {
        // already past the switch point
        0.0
    } else if charge_now_uah < switch_uah {
        // still in CC: estimate time to reach the switch point linearly
        let cc_remaining = switch_uah - charge_now_uah;
        let effective_current = if current_ua > 0.0 {
            current_ua
        } else {
            profile.cc_current_ua
        };
        if effective_current <= 0.0 {
            return None;
        }
        cc_remaining / effective_current * 3600.0
    } else {
        0.0
    };

    // CV portion: charge to deposit from switch_uah to charge_full_uah
    let cv_charge_uas = (charge_full_uah - switch_uah.max(charge_now_uah)) * 3600.0;
    let tau = profile.cv_tau_secs;
    let i_start = profile.cv_start_current_ua;
    if tau <= 0.0 || i_start <= 0.0 {
        return None;
    }

    let deliverable = i_start * tau;
    if cv_charge_uas >= deliverable {
        return None;
    }

    let cv_secs = -tau * (1.0 - cv_charge_uas / deliverable).ln();
    Some(Duration::from_secs_f64(cc_secs + cv_secs))
}

// ── legacy stub (kept for history.rs delegation) ─────────────────────────────

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
