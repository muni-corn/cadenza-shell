//! CC/CV charging phase detection and time-to-full prediction.
//!
//! Learned per-device charging parameters ([`ChargeProfile`]) are persisted to
//! disk and updated at the end of each charging session. The active session is
//! tracked in-memory by [`ChargingSession`].

use std::{fs, path::PathBuf, time::Duration};

use anyhow::{Context, Result};
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

use super::{discharging::get_state_directory, sysfs::SysfsReading};

/// Learning rate for exponential moving averages applied to [`ChargeProfile`]
/// fields after each completed charging session.
const SESSION_LEARNING_RATE: f64 = 0.2;

/// The maximum number of readings that will count towards `tau`.
const MAX_TAU_READINGS: u32 = 1000;

/// Learned CC/CV charging parameters for this device.
///
/// All fields are updated via exponential moving average after each charging
/// session that contains a detectable CC-to-CV transition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChargeProfile {
    /// Average current during the CC (constant-current) phase, in microamperes.
    pub cc_plateau_ua: f64,

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

    /// Number of `tau` samples taken.
    pub tau_sample_count: u32,
}

impl Default for ChargeProfile {
    fn default() -> Self {
        Self {
            cc_plateau_ua: 0.0,
            switch_percentage: 0.0,
            cv_tau_secs: 0.0,
            cv_start_current_ua: 0.0,
            sessions_learned: 0,
            tau_sample_count: 0,
        }
    }
}

impl ChargeProfile {
    /// Update the profile with parameters learned from a completed charging
    /// session using an exponential moving average.
    ///
    /// - `cc_plateau_ua` – plateau current observed during the CC phase (µA).
    /// - `switch_pct` – state-of-charge fraction at which the CC→CV transition
    ///   was detected.
    /// - `cv_start_ua` – current at the moment of the CC→CV transition (µA).
    /// - `tau_secs` – fitted exponential decay constant for the CV phase (s).
    pub fn update_transition(&mut self, cc_plateau_ua: f64, switch_pct: f64, cv_start_ua: f64) {
        let alpha = SESSION_LEARNING_RATE;
        let one_minus = 1.0 - alpha;

        if self.sessions_learned == 0 {
            // first session: seed directly rather than blending with zeros
            self.cc_plateau_ua = cc_plateau_ua;
            self.switch_percentage = switch_pct;
            self.cv_start_current_ua = cv_start_ua;
        } else {
            self.cc_plateau_ua = self.cc_plateau_ua * one_minus + cc_plateau_ua * alpha;
            self.switch_percentage = self.switch_percentage * one_minus + switch_pct * alpha;
            self.cv_start_current_ua = self.cv_start_current_ua * one_minus + cv_start_ua * alpha;
        }

        self.sessions_learned += 1;
    }

    /// Records a value for `tau`. Learns a moving average of `tau` over at most
    /// `MAX_TAU_READINGS`.
    pub fn update_tau(&mut self, new_tau: f64) {
        self.tau_sample_count += 1;

        // the learning rate
        let alpha = 1.0 / (self.tau_sample_count.min(MAX_TAU_READINGS) as f64);

        self.cv_tau_secs = new_tau * alpha + self.cv_tau_secs * (1.0 - alpha);
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
        Some(Self {
            when: r.when,
            current_ua: r.current_now.unsigned_abs() as f64,
            percentage,
        })
    }
}

/// Number of readings in the rolling window used to compute the median current
/// for phase detection.
const PHASE_WINDOW: usize = 12;

/// How many consecutive readings below `CV_DROP_THRESHOLD` are required before
/// declaring the CC/CV transition.
const CV_CONFIRM_READINGS: usize = 5;

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

    /// Median current observed while in the CC phase (µA). Updated as
    /// long as `phase == Cc`.
    pub cc_plateau_ua: f64,

    /// Index into `readings` at which the CC→CV transition was detected.
    /// `None` if the transition has not yet been observed.
    pub reading_at_transition: Option<SessionReading>,

    /// Number of consecutive readings that have been below the CV drop
    /// threshold, used to confirm the transition before committing.
    cv_confirm_count: usize,

    /// CSV writer for per-reading session diagnostics. Initialized lazily on
    /// the first reading; `None` if the file could not be created.
    csv_writer: Option<csv::Writer<fs::File>>,
}

impl ChargingSession {
    /// Add a new reading and update the phase detection state.
    ///
    /// `profile` is consulted to provide a learned `switch_percentage` hint
    /// that can accelerate detection. Pass the default profile if none has
    /// been learned yet.
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

        self.write_csv_row();

        // need at least PHASE_WINDOW readings before making any determination
        if self.readings.len() < PHASE_WINDOW {
            return;
        }

        if let ChargingPhase::Unknown | ChargingPhase::Cc = self.phase {
            self.update_cc_phase(profile)
        }
    }

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
            "rolling_median_current_5",
            "rolling_median_current_10",
            "rolling_median_current_15",
            "rolling_median_current_20",
            "rolling_median_current_25",
            "rolling_median_current_30",
        ])?;
        writer.flush()?;
        log::info!("started charging session csv at {path:?}");
        Ok(writer)
    }

    /// Append one diagnostic row for the most recent reading.
    fn write_csv_row(&mut self) {
        let Some(latest) = self.readings.last() else {
            return;
        };

        let when = latest.when.to_rfc3339();
        let percentage = latest.percentage * 100.0;
        let current_now = latest.current_ua;
        let session_median = self.median_current();
        let rolling_current_5 = self.rolling_median_current(5);
        let rolling_current_10 = self.rolling_median_current(10);
        let rolling_current_15 = self.rolling_median_current(15);
        let rolling_current_20 = self.rolling_median_current(20);
        let rolling_current_25 = self.rolling_median_current(25);
        let rolling_current_30 = self.rolling_median_current(30);

        let Some(writer) = self.csv_writer.as_mut() else {
            return;
        };
        let row = [
            when,
            format!("{percentage:.2}"),
            format!("{current_now:.0}"),
            format!("{session_median:.0}"),
            format!("{rolling_current_5:.0}"),
            format!("{rolling_current_10:.0}"),
            format!("{rolling_current_15:.0}"),
            format!("{rolling_current_20:.0}"),
            format!("{rolling_current_25:.0}"),
            format!("{rolling_current_30:.0}"),
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
            "----------charging statistics---------
       percentage: {:.1}%
 plateau (median): {median} µA
 rolling_median_5: {rolling_median_5} µA
rolling_median_10: {rolling_median_10} µA
rolling_median_15: {rolling_median_15} µA
rolling_median_20: {rolling_median_20} µA
rolling_median_25: {rolling_median_25} µA
rolling_median_30: {rolling_median_30} µA",
            latest.percentage * 100.
        );

        // we determine the cc plateau as the median current of all readings. if that
        // currently differs while we're still in cc charging, update it
        if median != self.cc_plateau_ua {
            self.cc_plateau_ua = median;
        }

        if self.phase == ChargingPhase::Unknown && self.cc_plateau_ua > 0.0 {
            self.phase = ChargingPhase::Cc;
        }

        // use the learned switch percentage as a gating condition: don't start looking
        // for the transition until we are within 5% of the known point
        let in_switch_range = latest.percentage >= (profile.switch_percentage - 0.05).max(0.0);
        if !in_switch_range {
            log::debug!("not in switch range; not checking rolling median order");
            return;
        }

        log::debug!("in cc/cv switch range; checking now");

        // check whether all medians and instant current are ordered. when they are,
        // we're very likely in cv charging
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
            // reset confirmation streak on any reading above the threshold
            log::debug!("rolling medians are *not* in order; resetting");
            self.cv_confirm_count = 0;
        }

        if self.cv_confirm_count >= CV_CONFIRM_READINGS {
            log::debug!(
                "rolling medians have been ordered for {} readings; cv phase detected!",
                self.cv_confirm_count
            );

            // transition confirmed: mark it at the first reading of the streak
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

    // ── lifecycle ─────────────────────────────────────────────────────────────

    /// Finalise the session and, if a CC→CV transition was observed and the CV
    /// fit is valid, update `profile` with the learned parameters.
    ///
    /// Call this when the charger is disconnected or the status changes away
    /// from `Charging`.
    pub fn end(&self, profile: &mut ChargeProfile) {
        let Some(ref rat) = self.reading_at_transition else {
            log::debug!("session ended without a recorded cc/cv transition; profile unchanged");
            return;
        };

        let i0 = rat.current_ua;
        let tau = self.tau_secs().unwrap_or(0.0);
        let switch_pct = rat.percentage;
        let cc_plateau_ua = self.cc_plateau_ua;

        log::info!(
            "charging session complete: cc={cc_plateau_ua:.0} µA, \
             switch={:.1}%, tau={tau:.0} s, I₀={i0:.0} µA",
            switch_pct * 100.0,
        );

        profile.update_transition(cc_plateau_ua, switch_pct, i0);

        if let Err(e) = profile.save() {
            log::error!("couldn't save charge profile: {e}");
        }
    }

    // ── internal helpers ─────────────────────────────────────────────────────

    /// Compute the median current (µA) over the most recent number of
    /// `readings`. Uses a sorted copy, so it is robust to transient spikes
    /// and the load-induced dips visible in real data.
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

    /// Computes the constant `tau` for our exponential prediction curve, using
    /// only the latest reading and the reading recorded at the CV transition.
    pub fn tau_secs(&self) -> Option<f64> {
        let rat = &self.reading_at_transition.as_ref()?;
        let latest_reading = self.readings.last()?;

        let t0 = rat.when;
        let delta_t = (latest_reading.when - t0).num_seconds() as f64;

        // new exponential parameter `tau`
        Some(-delta_t / (latest_reading.current_ua / rat.current_ua))
    }

    // ── prediction ───────────────────────────────────────────────────────────────

    /// Predict time to full using the CC/CV model.
    ///
    /// Uses a three-tier strategy:
    ///
    /// 1. **Best** — active session has a confirmed CV phase with a valid OLS
    ///    fit: integrate the fitted exponential to the `charge_full_uah`
    ///    target.
    /// 2. **Good** — a session is active in CC or early CV, and a learned
    ///    [`ChargeProfile`] is available: combine a linear CC estimate with the
    ///    profile's CV time constant.
    /// 3. **Fallback** — no useful data yet: simple `charge_remaining /
    ///    current` linear extrapolation.
    ///
    /// Returns [`Duration::MAX`] when the battery is already full or no current
    /// is flowing.
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

        // tier 1: active CV fit
        if self.phase == ChargingPhase::Cv {
            match self.predict_cv_remaining(charge_now_uah, charge_full_uah) {
                Ok(t) => return t,
                Err(e) => log::error!("couldn't predict with cv model: {e}"),
            }
        }

        // tier 2: CC phase with a learned profile
        if let Some(t) =
            predict_cc_plus_cv(self, profile, current_ua, charge_now_uah, charge_full_uah)
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
    /// The amount of charge deposited between `t_now` and some future time `T`
    /// is:
    /// ```text
    /// Q = I₀ · tau · (exp(−t_now/tau) − exp(−T/tau))
    /// ```
    /// Solving for `T − t_now` gives:
    /// ```text
    /// Δt = −tau · ln(1 − Q / (I(t_now) · tau))
    /// ```
    ///
    /// `cc_plateau_ua` should be the session's observed plateau when available,
    /// falling back to the learned profile value so a mid-charge restart does
    /// not produce a near-zero `i_term`.
    fn predict_cv_remaining(&self, charge_now_uah: f64, charge_full_uah: f64) -> Result<Duration> {
        let tau = self.tau_secs().ok_or(anyhow::anyhow!(
            "a `tau` could not be computed from the current charging session"
        ))?;
        let i0 = self
            .reading_at_transition
            .as_ref()
            .map(|r| r.current_ua)
            .ok_or(anyhow::anyhow!(
                "there is no transition point recorded for this session"
            ))?;
        let rat = self
            .reading_at_transition
            .as_ref()
            .ok_or(anyhow::anyhow!("there is no known cv transition point"))?;

        // elapsed seconds since the CV transition started
        let t0 = rat.when;
        let t_now = self
            .readings
            .last()
            .ok_or(anyhow::anyhow!("there are no readings yet"))?
            .when;
        let t_elapsed = (t_now - t0).num_milliseconds() as f64 / 1000.0;

        // current value on the fitted curve at t_now (µA)
        let i_now_fitted = i0 * (-t_elapsed / tau).exp();

        // charge remaining (µAh → µAs for integration, then back)
        let charge_remaining_uas = (charge_full_uah - charge_now_uah) * 3600.0;

        // amount the exponential can ever deliver from t_now: I_now_fitted · tau
        let deliverable = i_now_fitted * tau;
        if charge_remaining_uas >= deliverable {
            // the exponential asymptote falls short of charge_full; don't use
            anyhow::bail!("exponential asymptote doesn't reach `charge_full`")
        }

        let delta_t_secs = -tau * (1.0 - charge_remaining_uas / deliverable).ln();
        Ok(Duration::from_secs_f64(delta_t_secs))
    }
}

fn median_of(iter: impl Iterator<Item = f64>) -> f64 {
    let mut values = iter.collect::<Vec<_>>();

    // return early if there are no values to work with
    if values.is_empty() {
        return 0.0;
    }

    // sort all
    values.sort_by(f64::total_cmp);

    let mid = values.len() / 2;
    if values.len().is_multiple_of(2) {
        // average the two middle values
        (values[mid - 1] + values[mid]) / 2.0
    } else {
        // the middle value
        values[mid]
    }
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
            profile.cc_plateau_ua
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

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use chrono::Local;

    use super::*;

    // ── helpers ───────────────────────────────────────────────────────────────

    /// Build a synthetic [`SessionReading`] at `t_offset_secs` seconds after
    /// `base`, with the given current and percentage.
    fn make_reading(
        base: DateTime<Local>,
        t_offset_secs: i64,
        current_ua: f64,
        percentage: f64,
    ) -> SessionReading {
        let when = base + chrono::Duration::seconds(t_offset_secs);
        SessionReading {
            when,
            current_ua,
            percentage,
        }
    }

    /// Push `n` identical CC readings at 10 s intervals, then return the
    /// session.
    fn fill_cc_phase(n: usize, current_ua: f64) -> (ChargingSession, DateTime<Local>) {
        let base = Local::now();
        let profile = ChargeProfile::default();
        let mut session = ChargingSession::default();
        for i in 0..n {
            let r = make_reading(base, i as i64 * 10, current_ua, 0.5 + i as f64 * 0.001);
            session.push(r, &profile);
        }
        (session, base)
    }

    // ── phase detection tests ─────────────────────────────────────────────────

    #[test]
    fn phase_starts_unknown() {
        let session = ChargingSession::default();
        assert_eq!(session.phase, ChargingPhase::Unknown);
    }

    #[test]
    fn phase_transitions_to_cc_after_window() {
        let (session, _) = fill_cc_phase(PHASE_WINDOW + 1, 3_800_000.0);
        assert_eq!(session.phase, ChargingPhase::Cc);
    }

    #[test]
    fn phase_detects_cv_transition() {
        // build a session: PHASE_WINDOW CC readings at full current, then
        // CV_CONFIRM_READINGS + a few extra at low current
        let base = Local::now();
        let profile = ChargeProfile::default();
        let mut session = ChargingSession::default();
        let cc_current = 3_800_000.0_f64;

        // CC phase — enough to establish a plateau
        for i in 0..(PHASE_WINDOW + 4) {
            let r = make_reading(base, i as i64 * 10, cc_current, 0.50 + i as f64 * 0.005);
            session.push(r, &profile);
        }
        assert_eq!(session.phase, ChargingPhase::Cc);

        // CV phase — current drops to 50% of plateau.
        // the rolling median needs PHASE_WINDOW/2 + 1 readings to tip over, then
        // CV_CONFIRM_READINGS more to confirm; use PHASE_WINDOW + CV_CONFIRM_READINGS
        // to have a clear margin.
        let cc_count = PHASE_WINDOW + 4;
        let low_current = cc_current * 0.50; // well below 0.75 threshold
        for i in 0..(PHASE_WINDOW + CV_CONFIRM_READINGS) {
            let r = make_reading(
                base,
                (cc_count + i) as i64 * 10,
                low_current,
                0.70 + i as f64 * 0.005,
            );
            session.push(r, &profile);
        }

        assert_eq!(session.phase, ChargingPhase::Cv);
        assert!(session.reading_at_transition.is_some());
    }

    #[test]
    fn phase_does_not_falsely_trigger_on_transient_dip() {
        // a single dip below threshold should not trigger a transition
        let base = Local::now();
        let profile = ChargeProfile::default();
        let mut session = ChargingSession::default();
        let cc_current = 3_800_000.0_f64;
        let dip_current = cc_current * 0.30; // deep dip

        // establish CC plateau
        for i in 0..(PHASE_WINDOW + 4) {
            let r = make_reading(base, i as i64 * 10, cc_current, 0.50 + i as f64 * 0.002);
            session.push(r, &profile);
        }

        // single dip reading
        let offset = PHASE_WINDOW + 4;
        session.push(
            make_reading(base, offset as i64 * 10, dip_current, 0.56),
            &profile,
        );

        // should still be in CC: one dip is nowhere near CV_CONFIRM_READINGS
        assert_eq!(session.phase, ChargingPhase::Cc);
    }

    // ── predict_time_to_full tests
    // ────────────────────────────────────────────
}
