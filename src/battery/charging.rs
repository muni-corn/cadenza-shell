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
