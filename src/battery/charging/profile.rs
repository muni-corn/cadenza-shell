use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::battery::{
    charging::{
        CvFitParams,
        consts::{
            self, DEFAULT_AMPLITUDE_RATIO, DEFAULT_TAU1_SECS, DEFAULT_TAU2_SECS,
            I_CUT_DEFAULT_C_RATE, I_CUT_LEARNING_RATE,
        },
    },
    discharging::get_state_directory,
};

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
    pub device_key: String,
}

/// Used with serde to provide a default value.
pub fn default_tau1() -> f64 {
    DEFAULT_TAU1_SECS
}

/// Used with serde to provide a default value.
pub fn default_tau2() -> f64 {
    DEFAULT_TAU2_SECS
}

/// Used with serde to provide a default value.
pub fn default_amplitude_ratio() -> f64 {
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
    pub(super) fn update_transition(
        &mut self,
        cc_plateau_ua: f64,
        switch_pct: f64,
        final_fit: Option<CvFitParams>,
    ) {
        let alpha = consts::SESSION_LEARNING_RATE;
        let one_minus = 1.0 - alpha;

        log::debug!(
            "update_transition (session #{}): cc={cc_plateau_ua:.0} µA, switch={:.1}%, \
             final_fit={}",
            self.sessions_learned + 1,
            switch_pct * 100.0,
            final_fit.map_or("none".to_string(), |p| format!("{p:#?}")),
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
                    "seeding tau priors: [{p:?}], ratio={:.2}",
                    p.a / cc_plateau_ua,
                );
                self.tau1_prior_secs = p.tau1;
                self.tau2_prior_secs = p.tau2;
                self.amplitude_ratio = p.a / cc_plateau_ua;
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
                let bt = consts::TAU_PRIOR_LEARNING_RATE;
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
        log::debug!("profile after update: {self:?}");
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

    pub fn profile_filename(device_key: &str) -> String {
        if device_key.is_empty() {
            "charge_profile.json".to_string()
        } else {
            format!("charge_profile_{device_key}.json")
        }
    }

    pub fn get_path(device_key: &str) -> Result<PathBuf> {
        Ok(get_state_directory()?.join(Self::profile_filename(device_key)))
    }

    /// Load the [`ChargeProfile`] for the given device from disk, returning a
    /// default profile if no file exists yet.
    pub fn load(device_key: &str) -> Self {
        match Self::try_load(device_key) {
            Ok(mut p) => {
                p.device_key = device_key.to_string();
                log::info!("loaded charge profile for '{device_key}': {p:?}");
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

    pub fn try_load(device_key: &str) -> Result<Self> {
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
        log::debug!("saved charge profile to {path:?}: {self:?}");
        Ok(())
    }
}
