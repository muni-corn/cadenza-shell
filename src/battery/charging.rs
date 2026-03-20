//! CC/CV charging phase detection and time-to-full prediction.
//!
//! Learned per-device charging parameters ([`ChargeProfile`]) are persisted to
//! disk and updated at the end of each charging session. The active session is
//! tracked in-memory by [`ChargingSession`].
//!
//! # CV model
//!
//! During the CV phase, charging current is modelled as a double exponential:
//! ```text
//! I(t) = A · exp(−t/tau1) + (I0 − A) · exp(−t/tau2)
//! ```
//! The model is fit online using the Levenberg-Marquardt algorithm (see
//! [`super::cv_fit`]) and time-to-full is solved via bisection against a
//! device-learned cutoff current `I_cut`.

use std::{fs, path::PathBuf, time::Duration};

use anyhow::{Context, Result};
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

use super::{
    cv_fit::{CvFitParams, CvFitState, predict_cv_duration_from_integral},
    discharging::get_state_directory,
    sysfs::SysfsReading,
};

// ── constants ────────────────────────────────────────────────────────────────

/// Learning rate for EMA updates to [`ChargeProfile`] session parameters.
const SESSION_LEARNING_RATE: f64 = 0.2;

/// Learning rate for the I_cut EWMA update.
const I_CUT_LEARNING_RATE: f64 = 0.1;

/// Learning rate for tau prior EWMA updates.
const TAU_PRIOR_LEARNING_RATE: f64 = 0.15;

/// Cold-start I_cut fraction of full-charge capacity (`0.05C`).
const I_CUT_DEFAULT_C_RATE: f64 = 0.05;

/// Default fast time constant prior (seconds).
const DEFAULT_TAU1_SECS: f64 = 300.0;

/// Default slow time constant prior (seconds).
const DEFAULT_TAU2_SECS: f64 = 1_800.0;

/// Default amplitude ratio prior (A / I0).
const DEFAULT_AMPLITUDE_RATIO: f64 = 0.7;

// ── ChargeProfile
// ─────────────────────────────────────────────────────────────

/// Learned CC/CV charging parameters for this device.
///
/// All fields are updated via exponential moving average after each completed
/// charging session. Persisted to disk as JSON, keyed per device identity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChargeProfile {
    // ── CC phase ──────────────────────────────────────────────────────────────
    /// Average current during the CC (constant-current) phase (µA).
    pub cc_plateau_ua: f64,

    /// Battery state-of-charge fraction at which the CC-to-CV transition is
    /// typically observed (`[0, 1]`).
    pub switch_percentage: f64,

    /// Number of completed charging sessions that contributed to this profile.
    pub sessions_learned: u32,

    // ── CV phase priors ───────────────────────────────────────────────────────
    /// Fast time constant prior for the double-exponential CV model (s).
    #[serde(default = "default_tau1")]
    pub tau1_prior_secs: f64,

    /// Slow time constant prior for the double-exponential CV model (s).
    #[serde(default = "default_tau2")]
    pub tau2_prior_secs: f64,

    /// Amplitude ratio prior: A / I0, where A is the fast-decay amplitude.
    #[serde(default = "default_amplitude_ratio")]
    pub amplitude_ratio: f64,

    // ── learned termination current ───────────────────────────────────────────
    /// Learned termination current: the effective charging current at which
    /// this device typically transitions from CV to Full (µA).
    /// Zero means the cold-start `0.05C` prior is used.
    #[serde(default)]
    pub i_cut_ua: f64,

    /// Number of successfully observed full charge cycles contributing to
    /// `i_cut_ua`.
    #[serde(default)]
    pub i_cut_confidence: u32,

    // ── internal: runtime-only ────────────────────────────────────────────────
    /// Device key used to derive the storage filename. Not serialized.
    #[serde(skip)]
    device_key: String,
}

fn default_tau1() -> f64 {
    DEFAULT_TAU1_SECS
}

fn default_tau2() -> f64 {
    DEFAULT_TAU2_SECS
}

fn default_amplitude_ratio() -> f64 {
    DEFAULT_AMPLITUDE_RATIO
}

impl Default for ChargeProfile {
    fn default() -> Self {
        Self {
            cc_plateau_ua: 0.0,
            switch_percentage: 0.0,
            sessions_learned: 0,
            tau1_prior_secs: DEFAULT_TAU1_SECS,
            tau2_prior_secs: DEFAULT_TAU2_SECS,
            amplitude_ratio: DEFAULT_AMPLITUDE_RATIO,
            i_cut_ua: 0.0,
            i_cut_confidence: 0,
            device_key: String::new(),
        }
    }
}

impl ChargeProfile {
    /// Return the effective cutoff current for this device (µA).
    ///
    /// Returns the learned `i_cut_ua` when available; otherwise falls back to
    /// `0.05 · charge_full_uah` (0.05C rate) as a cold-start estimate.
    pub fn effective_i_cut(&self, charge_full_uah: f64) -> f64 {
        if self.i_cut_ua > 0.0 {
            log::debug!(
                "i_cut: using learned {:.0} µA (confidence: {} cycles)",
                self.i_cut_ua,
                self.i_cut_confidence,
            );
            self.i_cut_ua
        } else {
            // 0.05C: charge_full_uah × 0.05 gives µAh/h = µA
            let cold = I_CUT_DEFAULT_C_RATE * charge_full_uah;
            log::debug!(
                "i_cut: no learned value; using cold-start 0.05C = {cold:.0} µA \
                 (charge_full={:.1} mAh)",
                charge_full_uah / 1000.0,
            );
            cold
        }
    }

    // ── learning updates ──────────────────────────────────────────────────────

    /// Update the profile with parameters learned from a completed charging
    /// session using an exponential moving average.
    ///
    /// - `cc_plateau_ua` – plateau current observed during the CC phase (µA).
    /// - `switch_pct` – state-of-charge fraction at the CC→CV transition.
    /// - `final_fit` – the last valid double-exp fit from the session, if any.
    pub fn update_transition(
        &mut self,
        cc_plateau_ua: f64,
        switch_pct: f64,
        final_fit: Option<CvFitParams>,
    ) {
        let alpha = SESSION_LEARNING_RATE;
        let one_minus = 1.0 - alpha;

        log::debug!(
            "update_transition (session #{n}):
           cc = {cc:.0} µA
       switch = {sw:.1}%
    final_fit = {fit}",
            n = self.sessions_learned + 1,
            cc = cc_plateau_ua,
            sw = switch_pct * 100.0,
            fit = final_fit.map_or("none".to_string(), |p| {
                format!(
                    "[A={:.0} µA, tau1={:.0} s, tau2={:.0} s]",
                    p.a, p.tau1, p.tau2
                )
            }),
        );

        if self.sessions_learned == 0 {
            // first session: seed directly
            log::debug!(
                "first session: seeding cc_plateau={cc_plateau_ua:.0} µA, switch={:.1}%",
                switch_pct * 100.0,
            );
            self.cc_plateau_ua = cc_plateau_ua;
            self.switch_percentage = switch_pct;
            if let Some(p) = final_fit {
                log::debug!(
                    "seeding tau priors:
     tau1 = {:.0} s
     tau2 = {:.0} s
    ratio = {:.2}",
                    p.tau1,
                    p.tau2,
                    (p.a / cc_plateau_ua).clamp(0.1, 0.9),
                );
                self.tau1_prior_secs = p.tau1;
                self.tau2_prior_secs = p.tau2;
                self.amplitude_ratio = (p.a / cc_plateau_ua).clamp(0.1, 0.9);
            }
        } else {
            let prev_cc = self.cc_plateau_ua;
            let prev_sw = self.switch_percentage;
            self.cc_plateau_ua = self.cc_plateau_ua * one_minus + cc_plateau_ua * alpha;
            self.switch_percentage = self.switch_percentage * one_minus + switch_pct * alpha;
            log::debug!(
                "ema update (α={alpha:.2}):
    cc_plateau {prev_cc:.0}→{:.0}
    µA switch {:.1}%→{:.1}%",
                self.cc_plateau_ua,
                prev_sw * 100.0,
                self.switch_percentage * 100.0,
            );

            if let Some(p) = final_fit {
                let bt = TAU_PRIOR_LEARNING_RATE;
                let bm = 1.0 - bt;
                let prev_tau1 = self.tau1_prior_secs;
                let prev_tau2 = self.tau2_prior_secs;
                let prev_ratio = self.amplitude_ratio;
                self.tau1_prior_secs = self.tau1_prior_secs * bm + p.tau1 * bt;
                self.tau2_prior_secs = self.tau2_prior_secs * bm + p.tau2 * bt;
                if cc_plateau_ua > 0.0 {
                    let ratio = (p.a / cc_plateau_ua).clamp(0.1, 0.9);
                    self.amplitude_ratio = self.amplitude_ratio * bm + ratio * bt;
                }
                log::debug!(
                    "tau prior ema (β={bt:.2}):
    tau1 {prev_tau1:.0}→{:.0} s
    tau2 {prev_tau2:.0}→{:.0} s
    ratio {prev_ratio:.2}→{:.2}",
                    self.tau1_prior_secs,
                    self.tau2_prior_secs,
                    self.amplitude_ratio,
                );
            } else {
                log::debug!("  no final fit available; tau priors unchanged");
            }
        }

        self.sessions_learned += 1;
        log::debug!(
            "profile after update:
          cc = {:.0} µA
      switch = {:.1}%
        tau1 = {:.0} s
        tau2 = {:.0} s
       ratio = {:.2}
       i_cut = {:.0} µA (confidence = {})
    sessions = {}",
            self.cc_plateau_ua,
            self.switch_percentage * 100.0,
            self.tau1_prior_secs,
            self.tau2_prior_secs,
            self.amplitude_ratio,
            self.i_cut_ua,
            self.i_cut_confidence,
            self.sessions_learned,
        );
    }

    /// Update the learned termination current from an observed termination
    /// event (battery reached Full while in CV mode).
    ///
    /// Uses an EWMA with `I_CUT_LEARNING_RATE = 0.1`.
    pub fn update_i_cut(&mut self, observed_ua: f64) {
        let prev = self.i_cut_ua;
        if self.i_cut_ua <= 0.0 {
            // first observation: seed directly
            log::debug!("i_cut: first observation, seeding to {observed_ua:.0} µA");
            self.i_cut_ua = observed_ua;
        } else {
            self.i_cut_ua =
                self.i_cut_ua * (1.0 - I_CUT_LEARNING_RATE) + observed_ua * I_CUT_LEARNING_RATE;
            log::debug!(
                "i_cut ewma (β={:.2}): {prev:.0} → {:.0} µA  (observed={observed_ua:.0} µA)",
                I_CUT_LEARNING_RATE,
                self.i_cut_ua,
            );
        }
        self.i_cut_confidence += 1;
        log::info!(
            "i_cut updated: {:.0} µA (confidence: {} cycles)",
            self.i_cut_ua,
            self.i_cut_confidence,
        );
    }

    // ── persistence ───────────────────────────────────────────────────────────

    fn profile_filename(device_key: &str) -> String {
        if device_key.is_empty() {
            "charge_profile.json".to_string()
        } else {
            format!("charge_profile_{device_key}.json")
        }
    }

    fn get_path(device_key: &str) -> Result<PathBuf> {
        Ok(get_state_directory()?.join(Self::profile_filename(device_key)))
    }

    /// Load the [`ChargeProfile`] for the given device from disk, returning a
    /// default profile if no file exists yet.
    pub fn load(device_key: &str) -> Self {
        match Self::try_load(device_key) {
            Ok(mut p) => {
                p.device_key = device_key.to_string();
                log::info!(
                    "loaded charge profile for '{}' ({} sessions)
    cc_plateau={:.0} µA
    switch={:.1}%
    tau1={:.0} s
    tau2={:.0} s
    ratio={:.2}
    i_cut={:.0} µA (confidence={})",
                    device_key,
                    p.sessions_learned,
                    p.cc_plateau_ua,
                    p.switch_percentage * 100.0,
                    p.tau1_prior_secs,
                    p.tau2_prior_secs,
                    p.amplitude_ratio,
                    p.i_cut_ua,
                    p.i_cut_confidence,
                );
                p
            }
            Err(e) => {
                // also try the legacy single-file path for migration
                if let Ok(mut legacy) = Self::try_load("") {
                    log::info!(
                        "migrating legacy charge_profile.json to device key '{device_key}': {e}"
                    );
                    legacy.device_key = device_key.to_string();
                    // attempt to save under the new key immediately
                    if let Err(se) = legacy.save() {
                        log::warn!("couldn't save migrated profile: {se}");
                    }
                    return legacy;
                }
                log::info!("starting fresh charge profile for '{device_key}': {e}");
                Self {
                    device_key: device_key.to_string(),
                    ..Self::default()
                }
            }
        }
    }

    fn try_load(device_key: &str) -> Result<Self> {
        let path = Self::get_path(device_key)?;
        let json =
            fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        serde_json::from_str(&json).with_context(|| format!("parsing {}", path.display()))
    }

    /// Persist the profile to disk under the device-keyed filename.
    pub fn save(&self) -> Result<()> {
        let path = Self::get_path(&self.device_key)?;
        let json = serde_json::to_string_pretty(self)?;
        fs::write(&path, json).with_context(|| format!("writing {}", path.display()))?;
        log::debug!(
            "saved charge profile to {path:?}:
     sessions = {sessions}
           cc = {cc:.0} µA
       switch = {sw:.1}%
         tau1 = {tau1:.0} s
         tau2 = {tau2:.0} s
        ratio = {ratio:.2}
        i_cut = {i_cut:.0} µA (confidence = {conf})",
            sessions = self.sessions_learned,
            cc = self.cc_plateau_ua,
            sw = self.switch_percentage * 100.0,
            tau1 = self.tau1_prior_secs,
            tau2 = self.tau2_prior_secs,
            ratio = self.amplitude_ratio,
            i_cut = self.i_cut_ua,
            conf = self.i_cut_confidence,
        );
        Ok(())
    }
}

// ── ChargingPhase
// ─────────────────────────────────────────────────────────────

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
    /// as the battery approaches full.
    Cv,
}

// ── SessionReading
// ────────────────────────────────────────────────────────────

/// A single timestamped observation recorded during a charging session.
#[derive(Debug, Clone)]
pub struct SessionReading {
    /// Wall-clock time of this reading.
    pub when: DateTime<Local>,

    /// Measured current in microamperes (µA). Always positive (charging).
    pub current_ua: f64,

    /// State of charge as a fraction `[0, 1]`.
    pub percentage: f64,
}

impl SessionReading {
    /// Construct a [`SessionReading`] from a raw sysfs snapshot.
    ///
    /// Returns `None` if the sysfs reading lacks percentage data.
    pub fn from_sysfs(r: &SysfsReading) -> Option<Self> {
        let percentage = r.percentage()?;
        Some(Self {
            when: r.when,
            current_ua: r.current_now.unsigned_abs() as f64,
            percentage,
        })
    }
}

// ── ChargingSession constants
// ─────────────────────────────────────────────────

/// How many consecutive readings with ordered rolling medians are required
/// before declaring the CC/CV transition.
const CV_CONFIRM_READINGS: usize = 5;

/// Minimum number of pre-Full readings required to trust an I_cut observation.
const MIN_I_CUT_SAMPLES: usize = 3;

// ── ChargingSession
// ───────────────────────────────────────────────────────────

/// Transient state for the charging session that is currently in progress.
///
/// Created when a charger is connected and discarded when it is removed or the
/// battery reaches Full. Phase detection, online curve fitting and CV
/// prediction operate on the readings accumulated here.
#[derive(Debug, Default)]
pub struct ChargingSession {
    /// All readings collected since the charger was connected.
    pub readings: Vec<SessionReading>,

    /// Currently detected phase.
    pub phase: ChargingPhase,

    /// Median current observed while in the CC phase (µA). Updated as long as
    /// `phase == Cc`.
    pub cc_plateau_ua: f64,

    /// Reading recorded at the CC→CV transition.
    /// `None` if the transition has not yet been observed.
    pub reading_at_transition: Option<SessionReading>,

    /// Number of consecutive readings that have been below the CV drop
    /// threshold, used to confirm the transition before committing.
    cv_confirm_count: usize,

    /// Online double-exponential fitter. Initialized lazily when the CV phase
    /// is confirmed.
    cv_fit: Option<CvFitState>,

    /// CSV writer for per-reading session diagnostics. Initialized lazily on
    /// the first reading; `None` if the file could not be created.
    csv_writer: Option<csv::Writer<fs::File>>,
}

impl ChargingSession {
    /// Add a new reading and update phase detection and CV fitting state.
    ///
    /// `profile` is consulted for the learned `switch_percentage` hint and for
    /// CV model priors. Pass the default profile if none has been learned yet.
    pub fn push(&mut self, reading: SessionReading, profile: &ChargeProfile) {
        self.readings.push(reading);

        // lazily open the csv on the very first reading
        if self.csv_writer.is_none()
            && let Some(first) = self.readings.first()
        {
            let start_time = first.when;
            match self.try_open_csv(start_time) {
                Ok(writer) => self.csv_writer = Some(writer),
                Err(e) => log::error!("couldn't create charging session csv: {e}"),
            }
        }

        self.write_csv_row(profile);

        if let ChargingPhase::Unknown | ChargingPhase::Cc = self.phase {
            self.update_cc_phase(profile);
        }

        // drive the online CV fitter once we are in CV
        if self.phase == ChargingPhase::Cv {
            let latest = self.readings.last().unwrap(); // just pushed above
            let when = latest.when;
            let current_ua = latest.current_ua;
            let total_readings = self.readings.len();
            log::debug!(
                "cv phase update #{total_readings}:
      t = {:.0} s
      I = {current_ua:.0} µA
    soc = {:.1}%",
                self.cv_fit
                    .as_ref()
                    .map(|f| f.elapsed_secs(when))
                    .unwrap_or(0.0),
                latest.percentage * 100.0,
            );

            let fit = self.cv_fit.get_or_insert_with(|| {
                let rat = self.reading_at_transition.as_ref().unwrap();
                log::debug!(
                    "initialising cv fit state:
            i0 = {:.0} µA
    tau1_prior = {:.0} s
    tau2_prior = {:.0} s",
                    rat.current_ua,
                    profile.tau1_prior_secs,
                    profile.tau2_prior_secs,
                );
                CvFitState::new(
                    rat.current_ua,
                    rat.when,
                    profile.tau1_prior_secs,
                    profile.tau2_prior_secs,
                    profile.amplitude_ratio,
                )
            });

            fit.push_sample(when, current_ua);

            if fit.should_refit() {
                fit.refit();
            }
        }
    }

    // ── lifecycle ─────────────────────────────────────────────────────────────

    /// Finalise the session when charging stops for a reason other than Full
    /// (e.g. charger disconnected). Updates CC-phase parameters in `profile`
    /// if a CC→CV transition was observed; does NOT update I_cut.
    pub fn end(&self, profile: &mut ChargeProfile) {
        let Some(ref rat) = self.reading_at_transition else {
            log::debug!("session ended without a recorded cc/cv transition; profile unchanged");
            return;
        };

        let final_fit = self
            .cv_fit
            .as_ref()
            .filter(|f| f.has_valid_fit())
            .map(|f| f.params());

        log::info!(
            "charging session ended (not full):
        cc = {:.0} µA
    switch = {:.1}%",
            self.cc_plateau_ua,
            rat.percentage * 100.0,
        );

        profile.update_transition(self.cc_plateau_ua, rat.percentage, final_fit);

        if let Err(e) = profile.save() {
            log::error!("couldn't save charge profile: {e}");
        }
    }

    /// Finalise the session when the battery reaches Full.
    ///
    /// Updates CC-phase parameters and, if we were in CV mode with enough
    /// pre-Full samples, also updates the learned `I_cut` and tau priors.
    ///
    /// `charge_full_uah` is used to compute the cold-start I_cut prior for
    /// rejection sanity checks.
    pub fn end_full(&self, profile: &mut ChargeProfile, charge_full_uah: f64) {
        log::debug!(
            "end_full: {} total readings
           phase = {:?}
    cv_fit_valid = {}
      cv_samples = {}",
            self.readings.len(),
            self.phase,
            self.cv_fit.as_ref().is_some_and(|f| f.has_valid_fit()),
            self.cv_fit.as_ref().map(|f| f.sample_count()).unwrap_or(0),
        );

        // update CC-phase parameters if we have a transition
        if let Some(ref rat) = self.reading_at_transition {
            let final_fit = self
                .cv_fit
                .as_ref()
                .filter(|f| f.has_valid_fit())
                .map(|f| f.params());

            log::info!(
                "charging session complete (full): cc={:.0} µA, switch={:.1}%",
                self.cc_plateau_ua,
                rat.percentage * 100.0,
            );
            log::debug!(
                "final fit: {}",
                final_fit.map_or("none".to_string(), |p| {
                    format!(
                        "A = {:.0} µA, tau1 = {:.0} s, tau2 = {:.0} s",
                        p.a, p.tau1, p.tau2
                    )
                }),
            );

            profile.update_transition(self.cc_plateau_ua, rat.percentage, final_fit);
        } else {
            log::debug!("session reached full without a recorded cc/cv transition");
        }

        // attempt to observe I_cut from the readings just before Full
        if self.phase == ChargingPhase::Cv
            && let Some(i_term) = self.observe_i_cut(profile, charge_full_uah)
        {
            profile.update_i_cut(i_term);
        }

        if let Err(e) = profile.save() {
            log::error!("couldn't save charge profile: {e}");
        }
    }

    // ── prediction ───────────────────────────────────────────────────────────

    /// Predict time to full using the CC/CV model.
    ///
    /// Uses a three-tier strategy:
    ///
    /// 1. **Best** — active session is in CV with a valid double-exponential
    ///    fit: bisection on fitted curve against learned `I_cut`.
    /// 2. **Good** — session is in CC or early CV with a learned profile:
    ///    linear CC estimate plus double-exp CV integral from priors.
    /// 3. **Fallback** — no useful data yet: simple `charge_remaining /
    ///    current` linear extrapolation.
    ///
    /// Returns [`Duration::MAX`] when the battery is already full or no
    /// current is flowing.
    pub fn predict_time_to_full_cc_cv(
        &self,
        profile: &ChargeProfile,
        current_ua: f64,
        charge_now_uah: f64,
        charge_full_uah: f64,
    ) -> Duration {
        if charge_now_uah >= charge_full_uah || current_ua <= 0.0 {
            return Duration::MAX;
        }

        let charge_remaining_uah = charge_full_uah - charge_now_uah;

        log::debug!(
            "predict_time_to_full:
          phase = {:?}
              I = {:.0} µA
     charge_now = {:.1} mAh
    charge_full = {:.1} mAh
      remaining = {:.1} mAh",
            self.phase,
            current_ua,
            charge_now_uah / 1000.0,
            charge_full_uah / 1000.0,
            charge_remaining_uah / 1000.0,
        );

        // tier 1: active CV fit
        if self.phase == ChargingPhase::Cv {
            match self.predict_cv_remaining(profile, charge_full_uah) {
                Ok(t) => {
                    log::debug!(
                        "prediction tier 1 (cv fit): {:.1} min",
                        t.as_secs_f64() / 60.0,
                    );
                    return t;
                }
                Err(e) => log::warn!("tier-1 cv prediction failed: {e}"),
            }
        }

        // tier 2: CC phase with a learned profile
        if let Some(t) =
            predict_cc_plus_cv(self, profile, current_ua, charge_now_uah, charge_full_uah)
        {
            log::debug!(
                "prediction tier 2 (cc+cv profile): {:.1} min",
                t.as_secs_f64() / 60.0,
            );
            return t;
        }

        // tier 3: linear fallback
        let hours = charge_remaining_uah / current_ua;
        let t = Duration::from_secs_f64(hours * 3600.0);
        log::debug!(
            "prediction tier 3 (linear fallback): {:.1} min ({:.1} mAh / {:.0} µA)",
            t.as_secs_f64() / 60.0,
            charge_remaining_uah / 1000.0,
            current_ua,
        );
        t
    }

    // ── internal helpers ─────────────────────────────────────────────────────

    /// Open and return a new CSV writer for this session, writing the header.
    fn try_open_csv(&self, session_start: DateTime<Local>) -> Result<csv::Writer<fs::File>> {
        let dir = get_state_directory()?;
        let filename = format!(
            "charging_session_stats_{}.csv",
            session_start.format("%Y-%m-%dT%H-%M-%S")
        );
        let path = dir.join(&filename);
        let file = fs::File::create(&path).with_context(|| format!("creating {filename}"))?;
        let mut writer = csv::Writer::from_writer(file);
        writer.write_record([
            "when",
            "percentage",
            "instant_current",
            "session_median_current",
            "rolling_median_5",
            "rolling_median_10",
            "rolling_median_15",
            "rolling_median_20",
            "rolling_median_25",
            "rolling_median_30",
            "fit_a",
            "fit_tau1",
            "fit_tau2",
            "fit_valid",
        ])?;
        writer.flush()?;
        log::info!("started charging session csv at {path:?}");
        Ok(writer)
    }

    /// Append one diagnostic row for the most recent reading.
    fn write_csv_row(&mut self, _profile: &ChargeProfile) {
        let Some(latest) = self.readings.last() else {
            return;
        };

        let when = latest.when.to_rfc3339();
        let percentage = latest.percentage * 100.0;
        let current_now = latest.current_ua;
        let session_median = self.median_current();
        let r5 = self.rolling_median_current(5);
        let r10 = self.rolling_median_current(10);
        let r15 = self.rolling_median_current(15);
        let r20 = self.rolling_median_current(20);
        let r25 = self.rolling_median_current(25);
        let r30 = self.rolling_median_current(30);

        let (fit_a, fit_tau1, fit_tau2, fit_valid) = if let Some(f) = &self.cv_fit {
            let p = f.params();
            (p.a, p.tau1, p.tau2, f.has_valid_fit())
        } else {
            (0.0, 0.0, 0.0, false)
        };

        let Some(writer) = self.csv_writer.as_mut() else {
            return;
        };

        let row = [
            when,
            format!("{percentage:.2}"),
            format!("{current_now:.0}"),
            format!("{session_median:.0}"),
            format!("{r5:.0}"),
            format!("{r10:.0}"),
            format!("{r15:.0}"),
            format!("{r20:.0}"),
            format!("{r25:.0}"),
            format!("{r30:.0}"),
            format!("{fit_a:.0}"),
            format!("{fit_tau1:.1}"),
            format!("{fit_tau2:.1}"),
            format!("{}", fit_valid as u8),
        ];
        if let Err(e) = writer.write_record(&row) {
            log::error!("couldn't write charging session csv row: {e}");
            return;
        }
        if let Err(e) = writer.flush() {
            log::error!("couldn't flush charging session csv: {e}");
        }
    }

    fn update_cc_phase(&mut self, profile: &ChargeProfile) {
        let Some(latest) = self.readings.last() else {
            return;
        };
        let median = self.median_current();
        let rolling_median_5 = self.rolling_median_current(5);
        let rolling_median_10 = self.rolling_median_current(10);
        let rolling_median_15 = self.rolling_median_current(15);
        let rolling_median_20 = self.rolling_median_current(20);
        let rolling_median_25 = self.rolling_median_current(25);
        let rolling_median_30 = self.rolling_median_current(30);

        log::debug!(
            "
---------------charging statistics---------------
           percentage: {:.1}%
          current_now: {} µA
     rolling_median_5: {rolling_median_5} µA
    rolling_median_10: {rolling_median_10} µA
    rolling_median_15: {rolling_median_15} µA
    rolling_median_20: {rolling_median_20} µA
    rolling_median_25: {rolling_median_25} µA
    rolling_median_30: {rolling_median_30} µA
plateau (full median): {median} µA",
            latest.percentage * 100.,
            latest.current_ua,
        );

        // keep cc plateau updated while still in CC
        if median != self.cc_plateau_ua {
            self.cc_plateau_ua = median;
        }

        if self.phase == ChargingPhase::Unknown && self.cc_plateau_ua > 0.0 {
            self.phase = ChargingPhase::Cc;
        }

        // gate transition detection on the learned switch percentage
        let in_switch_range = latest.percentage >= (profile.switch_percentage - 0.05).max(0.0);
        if !in_switch_range {
            log::debug!("not in switch range; not checking rolling median order");
            return;
        }

        log::debug!("in cc/cv switch range; checking now");

        // all rolling medians must be strictly ordered for the transition to be
        // confirmed: instant < rolling_5 < ... < plateau
        let medians = [
            latest.current_ua,
            rolling_median_5,
            rolling_median_10,
            rolling_median_15,
            rolling_median_20,
            rolling_median_25,
            rolling_median_30,
            self.cc_plateau_ua,
        ];
        if medians
            .iter()
            .zip(medians.iter().skip(1))
            .all(|(a, b)| a < b)
        {
            log::debug!("rolling medians are in order; counting");
            self.cv_confirm_count += 1;
        } else {
            log::debug!("rolling medians are *not* in order; resetting");
            self.cv_confirm_count = 0;
        }

        if self.cv_confirm_count >= CV_CONFIRM_READINGS {
            log::debug!(
                "rolling medians have been ordered for {} readings; cv phase detected!",
                self.cv_confirm_count
            );

            let transition_idx = self.readings.len() - self.cv_confirm_count;
            self.phase = ChargingPhase::Cv;
            self.reading_at_transition = self.readings.get(transition_idx).cloned();
            if let Some(rat) = &self.reading_at_transition {
                log::info!(
                    "CC→CV transition detected at index {transition_idx} \
                     (soc={:.1}%, current={:.0} µA)",
                    rat.percentage * 100.0,
                    rat.current_ua,
                );
            }
        }
    }

    /// Tier 1: predict remaining time using the active double-exponential fit.
    ///
    /// Uses bisection on `I(t) = I_cut` where `I_cut` is the device-learned
    /// (or cold-start) termination current.
    fn predict_cv_remaining(
        &self,
        profile: &ChargeProfile,
        charge_full_uah: f64,
    ) -> Result<Duration> {
        let fit = self
            .cv_fit
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("cv fit state not initialised yet"))?;

        let i_cut = profile.effective_i_cut(charge_full_uah);
        let now = self
            .readings
            .last()
            .ok_or_else(|| anyhow::anyhow!("no readings in session"))?
            .when;

        log::debug!(
            "tier-1 cv predict:
  fit_valid = {}
buf_samples = {}
      i_cut = {:.0} µA",
            fit.has_valid_fit(),
            fit.sample_count(),
            i_cut,
        );

        fit.predict_time_remaining(now, i_cut)
            .ok_or_else(|| anyhow::anyhow!("model does not reach i_cut within prediction horizon"))
    }

    /// Observe the termination current from readings just before Full.
    ///
    /// Returns the median of the last `MIN_I_CUT_SAMPLES` positive readings,
    /// or `None` if there are not enough readings or the value is implausible.
    fn observe_i_cut(&self, profile: &ChargeProfile, charge_full_uah: f64) -> Option<f64> {
        let recent: Vec<f64> = self
            .readings
            .iter()
            .rev()
            .take(5)
            .filter(|r| r.current_ua > 0.0)
            .map(|r| r.current_ua)
            .collect();

        log::debug!(
            "observe_i_cut: {} pre-full positive readings: {:?}",
            recent.len(),
            recent.iter().map(|&i| i as u64).collect::<Vec<_>>(),
        );

        if recent.len() < MIN_I_CUT_SAMPLES {
            log::debug!(
                "not enough pre-full readings to observe i_cut ({} < {})",
                recent.len(),
                MIN_I_CUT_SAMPLES,
            );
            return None;
        }

        let mut sorted = recent.clone();
        sorted.sort_by(f64::total_cmp);
        let i_term = sorted[sorted.len() / 2];
        log::debug!(
            "observe_i_cut: sorted={:?}, median={i_term:.0} µA",
            sorted.iter().map(|&i| i as u64).collect::<Vec<_>>()
        );

        // sanity: must be positive and not an impossible jump from prior
        if i_term <= 0.0 {
            return None;
        }
        if profile.i_cut_ua > 0.0 {
            let ratio = i_term / profile.i_cut_ua;
            if !(0.1..=10.0).contains(&ratio) {
                log::warn!(
                    "i_cut observation rejected: {i_term:.0} µA is implausible \
                     given learned {:.0} µA",
                    profile.i_cut_ua,
                );
                return None;
            }
        }

        // sanity: must be a reasonable fraction of full capacity
        let cold_start = profile.effective_i_cut(charge_full_uah);
        if i_term > cold_start * 20.0 {
            log::warn!(
                "i_cut observation rejected: {i_term:.0} µA >> 20×cold-start {cold_start:.0} µA"
            );
            return None;
        }

        log::info!(
            "observed i_cut: {i_term:.0} µA (from {} pre-full readings)",
            recent.len()
        );
        Some(i_term)
    }

    /// Compute the median current (µA) over the most recent `readings` count.
    fn rolling_median_current(&self, readings: usize) -> f64 {
        let window_start = self.readings.len().saturating_sub(readings);
        let values = self.readings[window_start..].iter().map(|r| r.current_ua);
        median_of(values)
    }

    /// Compute the median current (µA) over all readings.
    fn median_current(&self) -> f64 {
        let values = self.readings.iter().map(|r| r.current_ua);
        median_of(values)
    }
}

// ── utility
// ───────────────────────────────────────────────────────────────────

fn median_of(iter: impl Iterator<Item = f64>) -> f64 {
    let mut values = iter.collect::<Vec<_>>();

    if values.is_empty() {
        return 0.0;
    }

    values.sort_by(f64::total_cmp);

    let mid = values.len() / 2;
    if values.len().is_multiple_of(2) {
        (values[mid - 1] + values[mid]) / 2.0
    } else {
        values[mid]
    }
}

// ── tier 2 prediction
// ─────────────────────────────────────────────────────────

/// Tier 2: linear CC estimate to the switch point, then double-exponential CV
/// estimate using the profile's learned priors.
fn predict_cc_plus_cv(
    session: &ChargingSession,
    profile: &ChargeProfile,
    current_ua: f64,
    charge_now_uah: f64,
    charge_full_uah: f64,
) -> Option<Duration> {
    let switch_uah = profile.switch_percentage * charge_full_uah;

    log::debug!(
        "tier-2 cc+cv:
     phase = {:?}
switch_soc = {:.1}% ({:.1} mAh)
cc_plateau = {:.0} µA
      tau1 = {:.0} s
      tau2 = {:.0} s
     ratio = {:.2}",
        session.phase,
        profile.switch_percentage * 100.0,
        switch_uah / 1000.0,
        profile.cc_plateau_ua,
        profile.tau1_prior_secs,
        profile.tau2_prior_secs,
        profile.amplitude_ratio,
    );

    let cc_secs = if session.phase == ChargingPhase::Cv {
        log::debug!("tier-2: already in CV, cc_secs=0");
        0.0
    } else if charge_now_uah < switch_uah {
        let cc_remaining = switch_uah - charge_now_uah;
        let effective_current = if current_ua > 0.0 {
            current_ua
        } else {
            profile.cc_plateau_ua
        };
        if effective_current <= 0.0 {
            log::debug!("tier-2: no usable CC current; bailing");
            return None;
        }
        let secs = cc_remaining / effective_current * 3600.0;
        log::debug!(
            "tier-2: CC remaining = {:.1} mAh at {:.0} µA → {:.1} min",
            cc_remaining / 1000.0,
            effective_current,
            secs / 60.0,
        );
        secs
    } else {
        log::debug!("tier-2: already past switch point, cc_secs=0");
        0.0
    };

    // CV portion: charge to deliver from the switch point to full
    let cv_start_uah = switch_uah.max(charge_now_uah);
    let cv_charge_uas = (charge_full_uah - cv_start_uah) * 3600.0;
    if cv_charge_uas <= 0.0 {
        log::debug!("tier-2: no CV charge needed; returning cc_secs only");
        return Some(Duration::from_secs_f64(cc_secs));
    }

    // use profile CC plateau as I0 proxy (CC plateau ≈ CV start current)
    let i0 = profile.cc_plateau_ua;
    if i0 <= 0.0 {
        log::debug!("tier-2: no learned CC plateau; bailing");
        return None;
    }

    let params = CvFitParams {
        a: profile.amplitude_ratio * i0,
        tau1: profile.tau1_prior_secs,
        tau2: profile.tau2_prior_secs,
    };

    if !params.is_valid(i0) {
        log::debug!(
            "tier-2: prior params invalid for i0 = {i0:.0} µA; bailing
    priors:
        A = {:.0}
        tau1 = {:.0}
        tau2 = {:.0}",
            params.a,
            params.tau1,
            params.tau2,
        );
        return None;
    }

    log::debug!(
        "tier-2: CV charge needed = {:.1} mAh
    i0 = {i0:.0} µA
    params:
        A = {:.0} µA
        tau1 = {:.0} s
        tau2 = {:.0} s",
        cv_charge_uas / 3600.0 / 1000.0,
        params.a,
        params.tau1,
        params.tau2,
    );

    let cv_duration = predict_cv_duration_from_integral(&params, i0, cv_charge_uas)?;
    let total = Duration::from_secs_f64(cc_secs) + cv_duration;
    log::debug!(
        "tier-2 result:
    cc = {:.1} min
    cv = {:.1} min
    total = {:.1} min",
        cc_secs / 60.0,
        cv_duration.as_secs_f64() / 60.0,
        total.as_secs_f64() / 60.0,
    );
    Some(total)
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use chrono::Local;

    use super::*;

    // ── phase detection tests ─────────────────────────────────────────────────

    #[test]
    fn phase_starts_unknown() {
        let session = ChargingSession::default();
        assert_eq!(session.phase, ChargingPhase::Unknown);
    }

    // ── ChargeProfile tests ───────────────────────────────────────────────────

    #[test]
    fn charge_profile_effective_i_cut_cold_start() {
        let profile = ChargeProfile::default();
        // with i_cut_ua = 0, should return 0.05 * charge_full_uah
        let charge_full_uah = 4_000_000.0;
        let expected = I_CUT_DEFAULT_C_RATE * charge_full_uah;
        assert!((profile.effective_i_cut(charge_full_uah) - expected).abs() < 1.0);
    }

    #[test]
    fn charge_profile_effective_i_cut_learned() {
        let profile = ChargeProfile {
            i_cut_ua: 150_000.0,
            ..ChargeProfile::default()
        };
        assert!((profile.effective_i_cut(4_000_000.0) - 150_000.0).abs() < 1.0);
    }

    #[test]
    fn charge_profile_update_i_cut_first_observation() {
        let mut profile = ChargeProfile::default();
        profile.update_i_cut(120_000.0);
        assert_eq!(profile.i_cut_ua, 120_000.0);
        assert_eq!(profile.i_cut_confidence, 1);
    }

    #[test]
    fn charge_profile_update_i_cut_ewma() {
        let mut profile = ChargeProfile::default();
        profile.update_i_cut(100_000.0);
        profile.update_i_cut(200_000.0);
        // EWMA: 100_000 * 0.9 + 200_000 * 0.1 = 110_000
        assert!((profile.i_cut_ua - 110_000.0).abs() < 1.0);
        assert_eq!(profile.i_cut_confidence, 2);
    }

    // ── double-exp prediction tests ───────────────────────────────────────────

    #[test]
    fn cv_fit_state_predicts_within_reasonable_bounds() {
        use super::super::cv_fit::CvFitState;

        let base = Local::now();
        let i0 = 3_000_000.0_f64; // 3 A

        let mut fit = CvFitState::new(i0, base, 300.0, 1800.0, 0.7);

        // feed synthetic double-exp samples: A=2.1M, tau1=400, tau2=2000
        let a = 0.7 * i0;
        let tau1 = 400.0_f64;
        let tau2 = 2_000.0_f64;
        for i in 1..=30 {
            let t = i as f64 * 20.0; // 20 s intervals
            let current = a * (-t / tau1).exp() + (i0 - a) * (-t / tau2).exp();
            let when = base + chrono::Duration::seconds(i * 20);
            fit.push_sample(when, current);
        }
        fit.refit();

        let now = base + chrono::Duration::seconds(30 * 20);
        let i_cut = i0 * 0.05; // 5% of i0
        let remaining = fit.predict_time_remaining(now, i_cut);
        // should give a finite positive estimate
        assert!(remaining.is_some());
        let secs = remaining.unwrap().as_secs_f64();
        assert!(secs > 0.0 && secs < 3.0 * tau2);
    }
}
