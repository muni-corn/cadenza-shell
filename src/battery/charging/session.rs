use std::{fs, time::Duration};

use anyhow::{Context, Result};
use chrono::{DateTime, Local};

use super::{
    super::sysfs::SysfsReading, are_medians_strictly_ordered, consts::ROLLING_WINDOWS,
    predict_cc_plus_cv, profile::ChargeProfile,
};
use crate::{
    battery::{
        charging::{
            ChargingPhase, CvFitState,
            consts::{CV_CONFIRM_READINGS, MIN_I_CUT_SAMPLES},
        },
        discharging::get_state_directory,
    },
    utils::median_of,
};

/// Transient state for the charging session that is currently in progress.
///
/// Created when a charger is connected and discarded when it is removed or
/// the battery reaches Full. Phase detection, online curve fitting and
/// CV prediction operate on the readings accumulated here.
#[derive(Debug, Default)]
pub struct ChargingSession {
    /// All readings collected since the charger was connected.
    pub readings: Vec<SessionReading>,

    /// Currently detected phase.
    pub phase: ChargingPhase,

    /// Median current observed while in the CC phase (µA). Updated as long
    /// as `phase == Cc`.
    pub cc_plateau_ua: f64,

    /// Reading recorded at the CC→CV transition.
    /// `None` if the transition has not yet been observed.
    pub reading_at_transition: Option<SessionReading>,

    /// Number of consecutive readings that have been below the CV drop
    /// threshold, used to confirm the transition before committing.
    cv_confirm_count: usize,

    /// Online double-exponential fitter. Initialized lazily when the CV
    /// phase is confirmed.
    pub(super) cv_fit: Option<CvFitState>,

    /// CSV writer for per-reading session diagnostics. Initialized lazily
    /// on the first reading; `None` if the file could not be
    /// created.
    csv_writer: Option<csv::Writer<fs::File>>,
}

impl ChargingSession {
    /// Add a new reading and update phase detection and CV fitting state.
    ///
    /// `profile` is consulted for the learned `switch_percentage` hint and
    /// for CV model priors. Pass the default profile if none has
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

        self.write_csv_row(profile);

        if let ChargingPhase::Unknown | ChargingPhase::Cc = self.phase {
            self.update_cc_phase(profile);
        }

        // drive the online CV fitter once we are in CV
        if self.phase == ChargingPhase::Cv {
            self.drive_cv_fit(profile);
        }
    }

    // ── lifecycle ─────────────────────────────────────────────────────────────

    /// Finalise the session when charging stops for a reason other than
    /// Full (e.g. charger disconnected). Updates CC-phase
    /// parameters in `profile` if a CC→CV transition was observed;
    /// does NOT update I_cut.
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
                final_fit.map_or("none".to_string(), |p| format!("{p:?}")),
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
            "predict_time_to_full: phase={:?} I={:.0} µA now={:.1} mAh full={:.1} mAh \
             remaining={:.1} mAh",
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

    /// Open and return a new CSV writer for this session, writing the
    /// header.
    pub(crate) fn try_open_csv(
        &self,
        session_start: DateTime<Local>,
    ) -> Result<csv::Writer<fs::File>> {
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
    pub(crate) fn write_csv_row(&mut self, _profile: &ChargeProfile) {
        let Some(latest) = self.readings.last() else {
            return;
        };

        let when = latest.when.to_rfc3339();
        let percentage = latest.percentage * 100.0;
        let current_now = latest.current_ua;
        let session_median = self.median_current();
        let [r5, r10, r15, r20, r25, r30] = self.rolling_medians();

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

    /// Drive the online CV fitter with the latest reading.
    pub(crate) fn drive_cv_fit(&mut self, profile: &ChargeProfile) {
        let latest = self.readings.last().unwrap(); // called only after push
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

    pub(crate) fn update_cc_phase(&mut self, profile: &ChargeProfile) {
        let Some(latest) = self.readings.last() else {
            return;
        };
        let median = self.median_current();
        let [r5, r10, r15, r20, r25, r30] = self.rolling_medians();

        log::debug!(
            "
---------------charging statistics---------------
           percentage: {:.1}%
          current_now: {} µA
     rolling_median_5: {r5} µA
    rolling_median_10: {r10} µA
    rolling_median_15: {r15} µA
    rolling_median_20: {r20} µA
    rolling_median_25: {r25} µA
    rolling_median_30: {r30} µA
plateau (full median): {median} µA",
            latest.percentage * 100.0,
            latest.current_ua,
        );

        self.cc_plateau_ua = median;

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

        self.update_cv_confirm(latest.current_ua, [r5, r10, r15, r20, r25, r30]);
        self.try_commit_cv_transition();
    }

    /// Update the CV confirmation counter based on whether the rolling
    /// medians are strictly ordered (instant < r5 < ... < r30 <
    /// plateau).
    pub(crate) fn update_cv_confirm(&mut self, instant_current: f64, rolling: [f64; 6]) {
        let check_medians = [
            instant_current,
            rolling[0],
            rolling[1],
            rolling[2],
            rolling[3],
            rolling[4],
            rolling[5],
            self.cc_plateau_ua,
        ];

        if are_medians_strictly_ordered(&check_medians) {
            log::debug!("rolling medians are in order; counting");
            self.cv_confirm_count += 1;
        } else {
            log::debug!("rolling medians are *not* in order; resetting");
            self.cv_confirm_count = 0;
        }
    }

    /// Commit the CC→CV transition if the confirmation threshold has been
    /// met.
    pub(crate) fn try_commit_cv_transition(&mut self) {
        if self.cv_confirm_count < CV_CONFIRM_READINGS {
            return;
        }

        log::debug!(
            "rolling medians have been ordered for {} readings; cv phase detected!",
            self.cv_confirm_count
        );

        let transition_idx = self.readings.len() - self.cv_confirm_count;
        self.phase = ChargingPhase::Cv;
        self.reading_at_transition = self.readings.get(transition_idx).cloned();
        if let Some(rat) = &self.reading_at_transition {
            log::info!(
                "CC→CV transition detected at index {transition_idx} (soc={:.1}%, current={:.0} \
                 µA)",
                rat.percentage * 100.0,
                rat.current_ua,
            );
        }
    }

    /// Tier 1: predict remaining time using the active double-exponential
    /// fit.
    ///
    /// Uses bisection on `I(t) = I_cut` where `I_cut` is the device-learned
    /// (or cold-start) termination current.
    pub(crate) fn predict_cv_remaining(
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
    /// Returns the median of the last `MIN_I_CUT_SAMPLES` positive
    /// readings, or `None` if there are not enough readings or the
    /// value is implausible.
    pub(crate) fn observe_i_cut(
        &self,
        profile: &ChargeProfile,
        charge_full_uah: f64,
    ) -> Option<f64> {
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
                    "i_cut observation rejected: {i_term:.0} µA is implausible given learned \
                     {:.0} µA",
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

    /// Compute rolling medians for each window in [`ROLLING_WINDOWS`].
    pub(crate) fn rolling_medians(&self) -> [f64; 6] {
        std::array::from_fn(|i| self.rolling_median_current(ROLLING_WINDOWS[i]))
    }

    /// Compute the median current (µA) over the most recent `readings`
    /// count.
    pub(crate) fn rolling_median_current(&self, readings: usize) -> f64 {
        let window_start = self.readings.len().saturating_sub(readings);
        let values = self.readings[window_start..].iter().map(|r| r.current_ua);
        median_of(values)
    }

    /// Compute the median current (µA) over all readings.
    pub(crate) fn median_current(&self) -> f64 {
        let values = self.readings.iter().map(|r| r.current_ua);
        median_of(values)
    }
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
