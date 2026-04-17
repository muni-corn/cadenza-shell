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

use std::time::Duration;

use cv_fit::{CvFitParams, CvFitState, predict_cv_duration_from_integral};
use profile::ChargeProfile;
use session::ChargingSession;

pub(super) mod profile;
pub(super) mod session;

mod consts;
mod cv_fit;

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

/// Returns `true` if each element in `medians` is strictly less than the next.
fn are_medians_strictly_ordered(medians: &[f64]) -> bool {
    medians
        .iter()
        .zip(medians.iter().skip(1))
        .all(|(a, b)| a < b)
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
    use crate::battery::charging::{consts::I_CUT_DEFAULT_C_RATE, session::ChargingSession};

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
        use super::cv_fit::CvFitState;

        let base = Local::now();
        let i0 = 3_000_000f64; // 3 A

        let mut fit = CvFitState::new(i0, base, 300.0, 1800.0, 0.7);

        // feed synthetic double-exp samples: A=2.1M, tau1=400, tau2=2000
        let a = 0.7 * i0;
        let tau1 = 400f64;
        let tau2 = 2_000f64;
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
