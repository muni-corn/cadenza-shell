//! Double-exponential CV phase fitting and time-to-full prediction.
//!
//! Models the charging current during the CV phase as:
//! ```text
//! I(t) = A * exp(-t / tau1) + (I0 - A) * exp(-t / tau2)
//! ```
//! where `I0` is the current at CV start, `A` is the fast-decay amplitude,
//! `tau1` is the fast time constant (s) and `tau2` is the slow time constant
//! (s).
//!
//! Fitting uses the Levenberg-Marquardt algorithm (warm-started from the
//! previous valid fit or profile priors). Time-to-full is solved via bisection
//! on the fitted curve against a learned cutoff current `I_cut`.

use std::{collections::VecDeque, time::Duration};

use chrono::{DateTime, Local};
use levenberg_marquardt::LeastSquaresProblem;
use nalgebra::{Dyn, OMatrix, OVector, U3, storage::Owned};

// ── constants ────────────────────────────────────────────────────────────────

/// Rolling buffer retention window for CV samples.
const CV_BUFFER_SECS: f64 = 900.0; // 15 minutes

/// Minimum new samples since last refit before triggering another refit.
const REFIT_SAMPLE_THRESHOLD: usize = 5;

/// Minimum wall-clock interval between refits (seconds).
const REFIT_INTERVAL_SECS: f64 = 45.0;

/// Number of CV samples at which the fit is considered stable enough to trust
/// fully.
const STABILIZATION_SAMPLES: usize = 8;

/// CV elapsed time (seconds) at which the fit is considered stable.
const STABILIZATION_DURATION_SECS: f64 = 120.0;

/// Tail-emphasis weight coefficient (λ in w_k = 1 + λ * t_k / t_end).
const WEIGHT_TAIL_EMPHASIS: f64 = 1.0;

/// Bisection convergence tolerance in seconds.
const BISECTION_TOLERANCE_SECS: f64 = 30.0;

/// Maximum bisection iterations.
const BISECTION_MAX_ITERS: usize = 64;

// ── data types ───────────────────────────────────────────────────────────────

/// A single current sample taken during the CV phase.
#[derive(Debug, Clone)]
pub(super) struct CvSample {
    /// Seconds elapsed since the CV phase started.
    pub t_secs: f64,
    /// Measured charging current (µA).
    pub current_ua: f64,
}

/// Double-exponential fit parameters.
#[derive(Debug, Clone, Copy)]
pub(super) struct CvFitParams {
    /// Fast-decay amplitude (µA). Must satisfy `0 < a < i0`.
    pub a: f64,
    /// Fast time constant (s). Must satisfy `tau1_min ≤ tau1 < tau2`.
    pub tau1: f64,
    /// Slow time constant (s). Must satisfy `tau1 < tau2 ≤ tau2_max`.
    pub tau2: f64,
}

impl CvFitParams {
    /// Evaluate `I(t) = A·exp(−t/tau1) + (I0−A)·exp(−t/tau2)`.
    pub fn eval(&self, t: f64, i0: f64) -> f64 {
        self.a * (-t / self.tau1).exp() + (i0 - self.a) * (-t / self.tau2).exp()
    }

    /// Returns `true` if the parameters are physically valid given `i0`.
    pub fn is_valid(&self, i0: f64) -> bool {
        self.a > 0.0 && self.a < i0 && self.tau2 > self.tau1
    }
}

// ── LM problem ───────────────────────────────────────────────────────────────

/// Nonlinear least-squares problem for the double-exponential CV model.
///
/// Parameters: `θ = [A, tau1, tau2]` (3-vector).
/// Residuals: `r_k = √w_k · (I_k − I_model(t_k))` for each sample k.
struct DoubleExpProblem {
    samples: Vec<CvSample>,
    weights: Vec<f64>,
    i0: f64,
    /// Current parameter vector `(A, tau1, tau2)`.
    p: (f64, f64, f64),
}

impl DoubleExpProblem {
    fn new(samples: &[CvSample], i0: f64, init: CvFitParams) -> Self {
        let t_end = samples.last().map(|s| s.t_secs).unwrap_or(1.0).max(1.0);

        let weights = samples
            .iter()
            .map(|s| 1.0 + WEIGHT_TAIL_EMPHASIS * (s.t_secs / t_end))
            .collect();

        Self {
            samples: samples.to_vec(),
            weights,
            i0,
            p: (init.a, init.tau1, init.tau2),
        }
    }

    #[inline]
    fn a(&self) -> f64 {
        self.p.0
    }

    #[inline]
    fn tau1(&self) -> f64 {
        self.p.1
    }

    #[inline]
    fn tau2(&self) -> f64 {
        self.p.2
    }

    fn model_at(&self, t: f64) -> f64 {
        self.a() * (-t / self.tau1()).exp() + (self.i0 - self.a()) * (-t / self.tau2()).exp()
    }
}

impl LeastSquaresProblem<f64, Dyn, U3> for DoubleExpProblem {
    type JacobianStorage = Owned<f64, Dyn, U3>;
    type ParameterStorage = Owned<f64, U3>;
    type ResidualStorage = Owned<f64, Dyn>;

    fn set_params(&mut self, x: &OVector<f64, U3>) {
        self.p = (x[0], x[1], x[2]);
    }

    fn params(&self) -> OVector<f64, U3> {
        OVector::<f64, U3>::new(self.p.0, self.p.1, self.p.2)
    }

    fn residuals(&self) -> Option<OVector<f64, Dyn>> {
        let v: Vec<f64> = self
            .samples
            .iter()
            .zip(self.weights.iter())
            .map(|(s, &w)| w.sqrt() * (s.current_ua - self.model_at(s.t_secs)))
            .collect();
        Some(OVector::<f64, Dyn>::from_vec(v))
    }

    fn jacobian(&self) -> Option<OMatrix<f64, Dyn, U3>> {
        let n = self.samples.len();
        let mut jac = OMatrix::<f64, Dyn, U3>::zeros(n);

        let a = self.a();
        let tau1 = self.tau1();
        let tau2 = self.tau2();
        let i0 = self.i0;

        for (k, (s, &w)) in self.samples.iter().zip(self.weights.iter()).enumerate() {
            let t = s.t_secs;
            let e1 = (-t / tau1).exp();
            let e2 = (-t / tau2).exp();
            let sw = w.sqrt();

            // negative because residual is (observed - model) and jacobian is d/dθ
            jac[(k, 0)] = -sw * (e1 - e2);
            jac[(k, 1)] = -sw * a * t / (tau1 * tau1) * e1;
            jac[(k, 2)] = -sw * (i0 - a) * t / (tau2 * tau2) * e2;
        }

        Some(jac)
    }
}

// ── fit state ────────────────────────────────────────────────────────────────

/// Online state for the double-exponential CV fit.
///
/// Maintains a rolling buffer of recent CV samples and refits the model
/// periodically using warm-started Levenberg-Marquardt minimization.
#[derive(Debug)]
pub(super) struct CvFitState {
    /// Wall time when the CV phase started.
    t0: DateTime<Local>,
    /// Charging current at CV start (µA). Fixed for the lifetime of this
    /// session.
    i0: f64,
    /// Rolling buffer of recent CV samples (at most `CV_BUFFER_SECS` old).
    samples: VecDeque<CvSample>,
    /// Prior parameters (from profile) used for stabilization blending.
    priors: CvFitParams,
    /// Best known fit parameters, updated after each successful refit.
    params: CvFitParams,
    /// Whether at least one valid fit has been accepted.
    has_valid_fit: bool,
    /// Number of samples added since the last refit attempt.
    samples_since_fit: usize,
    /// Wall time of the last refit attempt (successful or not).
    last_fit_at: Option<DateTime<Local>>,
    /// Total samples ever added (for stabilization tracking).
    total_samples: usize,
    /// Wall time of the first sample (for duration-based stabilization).
    first_sample_at: Option<DateTime<Local>>,
}

impl CvFitState {
    /// Create a new fit state at the start of a CV phase.
    ///
    /// - `i0` – charging current at the CV transition point (µA).
    /// - `t0` – wall time of the CV transition.
    /// - `tau1_prior`, `tau2_prior` – time constant priors from the profile.
    /// - `amplitude_ratio` – `A / I0` prior (e.g. 0.7).
    pub(super) fn new(
        i0: f64,
        t0: DateTime<Local>,
        tau1_prior: f64,
        tau2_prior: f64,
        amplitude_ratio: f64,
    ) -> Self {
        let priors = CvFitParams {
            a: amplitude_ratio * i0,
            tau1: tau1_prior,
            tau2: tau2_prior,
        };
        log::debug!(
            "cv fit state created: i0={i0:.0} µA, priors=[{priors:?}], amplitude_ratio={amplitude_ratio:.2}"
        );
        Self {
            t0,
            i0,
            samples: VecDeque::new(),
            priors,
            params: priors,
            has_valid_fit: false,
            samples_since_fit: 0,
            last_fit_at: None,
            total_samples: 0,
            first_sample_at: None,
        }
    }

    /// Push a new CV sample into the rolling buffer.
    pub(super) fn push_sample(&mut self, when: DateTime<Local>, current_ua: f64) {
        let t_secs = (when - self.t0).num_milliseconds() as f64 / 1000.0;

        if self.first_sample_at.is_none() {
            self.first_sample_at = Some(when);
        }

        self.samples.push_back(CvSample { t_secs, current_ua });
        self.samples_since_fit += 1;
        self.total_samples += 1;

        // evict samples older than the retention window
        let cutoff = t_secs - CV_BUFFER_SECS;
        let before_evict = self.samples.len();
        while self.samples.front().is_some_and(|s| s.t_secs < cutoff) {
            self.samples.pop_front();
        }
        let evicted = before_evict - self.samples.len();

        // model's expected current at this t for comparison
        let model_i = self.params.eval(t_secs, self.i0);
        let residual_pct = if model_i > 0.0 {
            (current_ua - model_i) / model_i * 100.0
        } else {
            0.0
        };

        log::debug!(
            "cv sample #{total}:
            t = {t:.0} s
            I = {I:.0} µA
        model = {model:.0} µA
     residual = {res:+.1}%
          buf = {buf}/{evicted_note}
    since_fit = {since}",
            total = self.total_samples,
            t = t_secs,
            I = current_ua,
            model = model_i,
            res = residual_pct,
            buf = self.samples.len(),
            evicted_note = if evicted > 0 {
                format!("evicted {evicted}")
            } else {
                "ok".to_string()
            },
            since = self.samples_since_fit,
        );
    }

    /// Returns `true` if a refit should be triggered now.
    pub(super) fn should_refit(&self) -> bool {
        if self.samples.len() < 3 {
            return false;
        }
        if self.samples_since_fit >= REFIT_SAMPLE_THRESHOLD {
            log::debug!(
                "refit triggered: {since} new samples >= threshold {thresh}",
                since = self.samples_since_fit,
                thresh = REFIT_SAMPLE_THRESHOLD,
            );
            return true;
        }
        if let Some(last) = self.last_fit_at {
            let elapsed = (Local::now() - last).num_seconds() as f64;
            if elapsed >= REFIT_INTERVAL_SECS {
                log::debug!(
                    "refit triggered: {elapsed:.0} s since last fit >= interval {interval:.0} s",
                    interval = REFIT_INTERVAL_SECS,
                );
                return true;
            }
        }
        false
    }

    /// Attempt to refit the double-exponential model to the current buffer.
    ///
    /// On success, updates the stored parameters. On failure (bad convergence
    /// or failed sanity checks), retains the previous valid parameters.
    pub(super) fn refit(&mut self) {
        let samples: Vec<CvSample> = self.samples.iter().cloned().collect();
        if samples.len() < 3 {
            return;
        }

        let alpha = self.stabilization_alpha();
        log::debug!(
            "lm refit: n={n} samples, span={t0:.0}..{t1:.0} s, warm=[{warm:?}], α={alpha:.2}",
            n = samples.len(),
            t0 = samples.first().map(|s| s.t_secs).unwrap_or(0.0),
            t1 = samples.last().map(|s| s.t_secs).unwrap_or(0.0),
            warm = self.params,
        );

        let problem = DoubleExpProblem::new(&samples, self.i0, self.params);
        let (result, report) = levenberg_marquardt::LevenbergMarquardt::new().minimize(problem);

        let p = result.params();
        let new_params = CvFitParams {
            a: p[0],
            tau1: p[1],
            tau2: p[2],
        };

        log::debug!(
            "lm finished: {:?} evals={} cost={:.4e} result=[{new_params:?}]",
            report.termination,
            report.number_of_evaluations,
            report.objective_function,
        );

        if self.validate_fit(&new_params) {
            let delta_a = new_params.a - self.params.a;
            let delta_tau1 = new_params.tau1 - self.params.tau1;
            let delta_tau2 = new_params.tau2 - self.params.tau2;
            log::debug!(
                "cv fit accepted: [{new_params:?}] \
                 (ΔA={delta_a:+.0}, Δτ1={delta_tau1:+.0} s, Δτ2={delta_tau2:+.0} s, had_valid={})",
                self.has_valid_fit,
            );
            self.params = new_params;
            self.has_valid_fit = true;
        } else {
            log::warn!(
                "cv fit rejected: A = {:.0} µA, tau1 = {:.0} s, tau2 = {:.0} s  \
                 (i0 = {:.0} µA, keeping previous params)",
                new_params.a,
                new_params.tau1,
                new_params.tau2,
                self.i0,
            );
        }

        self.samples_since_fit = 0;
        self.last_fit_at = Some(Local::now());
    }

    /// Returns `true` if at least one valid fit has been accepted.
    pub(super) fn has_valid_fit(&self) -> bool {
        self.has_valid_fit
    }

    /// The raw best-fit parameters (before stabilization blending).
    pub(super) fn params(&self) -> CvFitParams {
        self.params
    }

    /// Returns elapsed seconds from CV start to `when`.
    pub(super) fn elapsed_secs(&self, when: DateTime<Local>) -> f64 {
        (when - self.t0).num_milliseconds() as f64 / 1000.0
    }

    /// Number of samples currently in the rolling buffer.
    #[allow(dead_code)]
    pub(super) fn sample_count(&self) -> usize {
        self.samples.len()
    }

    /// Predict time remaining until `I(t) = i_cut` using bisection.
    ///
    /// Uses the effective parameters (blended with priors if insufficient
    /// data). Returns `None` if the model never reaches `i_cut` within a
    /// reasonable horizon, or if the current is already below `i_cut`.
    pub(super) fn predict_time_remaining(
        &self,
        now: DateTime<Local>,
        i_cut: f64,
    ) -> Option<Duration> {
        let p = self.effective_params();
        let t_now = self.elapsed_secs(now);
        let i_now = p.eval(t_now, self.i0);

        log::debug!(
            "predict_time_remaining: t_now={t_now:.0} s, I_now={i_now:.0} µA, \
             I_cut={i_cut:.0} µA, params=[{p:?}]"
        );

        // f(t) > 0 means model current is still above i_cut
        let f = |t: f64| p.eval(t, self.i0) - i_cut;

        if f(t_now) <= 0.0 {
            log::debug!("model already at/below i_cut ({i_now:.0} ≤ {i_cut:.0} µA); returning 0");
            return Some(Duration::ZERO);
        }

        // upper bound: 48 h
        let t_high = 48.0 * 3600.0_f64;
        if f(t_high) > 0.0 {
            log::debug!(
                "model does not reach i_cut ({i_cut:.0} µA) within {t_high:.0} s \
                 (I({t_high:.0}s) = {:.0} µA)",
                p.eval(t_high, self.i0),
            );
            return None;
        }

        // bisection to find t_cut in [t_now, t_high]
        let mut lo = t_now;
        let mut hi = t_high;
        for _ in 0..BISECTION_MAX_ITERS {
            let mid = (lo + hi) / 2.0;
            if f(mid) > 0.0 {
                lo = mid;
            } else {
                hi = mid;
            }
            if hi - lo < BISECTION_TOLERANCE_SECS {
                break;
            }
        }

        let t_cut = (lo + hi) / 2.0;
        let remaining = (t_cut - t_now).max(0.0);
        log::debug!(
            "bisection: t_cut={t_cut:.0} s, remaining={:.1} min, \
             I(t_cut)={:.0} µA ≈ i_cut={i_cut:.0} µA",
            remaining / 60.0,
            p.eval(t_cut, self.i0),
        );

        match Duration::try_from_secs_f64(t_cut) {
            Ok(d) => Some(d),
            Err(e) => {
                log::debug!("never mind, {e}; returning None (predict_time_remaining)");
                None
            }
        }
    }

    // ── private helpers ──────────────────────────────────────────────────────

    fn validate_fit(&self, p: &CvFitParams) -> bool {
        if !p.is_valid(self.i0) {
            log::debug!("fit invalid: [{p:?}] (A must be in 0..{:.0} µA)", self.i0);
            return false;
        }

        // model must be monotonically non-increasing over the sample range
        if let (Some(first), Some(last)) = (self.samples.front(), self.samples.back()) {
            let i_first = p.eval(first.t_secs, self.i0);
            let i_last = p.eval(last.t_secs, self.i0);
            if i_last > i_first * 1.01 {
                log::debug!(
                    "fit invalid: model increases over sample range (I({:.0}s) = {:.0} µA → I({:.0}s) = {:.0} µA)",
                    first.t_secs,
                    i_first,
                    last.t_secs,
                    i_last,
                );
                return false;
            }
        }

        // model shouldn't be wildly inconsistent with the most recent sample
        if let Some(last) = self.samples.back() {
            let i_pred = p.eval(last.t_secs, self.i0);
            let i_actual = last.current_ua;
            let discrepancy = (i_pred - i_actual).abs();
            let limit = 0.5 * self.i0;
            if discrepancy > limit {
                log::debug!(
                    "fit invalid: latest sample discrepancy too large (|{i_pred:.0} − {i_actual:.0}| = {discrepancy:.0} µA > limit {limit:.0} µA)",
                );
                return false;
            }
            log::debug!(
                "fit consistency ok:
     I_model = ({:.0}s)
     I_model = {i_pred:.0} µA
    I_actual = {i_actual:.0} µA
         |Δ| = {discrepancy:.0} µA",
                last.t_secs,
            );
        }

        true
    }

    /// Compute α for stabilization blending: 0 = fully prior, 1 = fully fit.
    fn stabilization_alpha(&self) -> f64 {
        let sample_alpha = (self.total_samples as f64 / STABILIZATION_SAMPLES as f64).min(1.0);

        let time_alpha = match self.first_sample_at {
            Some(first_at) => {
                let duration = (Local::now() - first_at).num_seconds() as f64;
                (duration / STABILIZATION_DURATION_SECS).min(1.0)
            }
            None => 0.0,
        };

        // both thresholds must be met
        sample_alpha.min(time_alpha)
    }

    /// Returns the effective parameters for prediction, blended with priors
    /// during the stabilization period.
    fn effective_params(&self) -> CvFitParams {
        let alpha = self.stabilization_alpha();
        if alpha >= 1.0 {
            return self.params;
        }
        let blend = |fit: f64, prior: f64| alpha * fit + (1.0 - alpha) * prior;
        let p = CvFitParams {
            a: blend(self.params.a, self.priors.a),
            tau1: blend(self.params.tau1, self.priors.tau1),
            tau2: blend(self.params.tau2, self.priors.tau2),
        };
        log::debug!(
            "effective params (α={alpha:.2}, stabilizing): [{p:?}] \
             (fit=[{fit:?}], prior=[{prior:?}])",
            fit = self.params,
            prior = self.priors,
        );
        p
    }
}

// ── charge-integral bisection (for tier 2 profile-based prediction) ──────────

/// Predict CV phase duration by finding when the charge integral under the
/// double-exponential curve equals `cv_charge_uas` (µA·s).
///
/// Returns `None` if the model cannot deliver the required charge.
pub(super) fn predict_cv_duration_from_integral(
    params: &CvFitParams,
    i0: f64,
    cv_charge_uas: f64,
) -> Option<Duration> {
    // total deliverable charge from t = 0 to ∞
    let q_total = params.a * params.tau1 + (i0 - params.a) * params.tau2;
    log::debug!(
        "cv integral prediction: Q_need={:.1} mAh, Q_max={:.1} mAh, params=[{params:?}]",
        cv_charge_uas / 3600.0 / 1000.0,
        q_total / 3600.0 / 1000.0,
    );
    if cv_charge_uas >= q_total {
        log::debug!(
            "integral prediction will never converge; giving up (Q_need = {:.1} mAh ≥ Q_max = {:.1} mAh)",
            cv_charge_uas / 3600.0 / 1000.0,
            q_total / 3600.0 / 1000.0,
        );
        return None;
    }

    // Q(T) = A*tau1*(1 − exp(−T/tau1)) + (I0−A)*tau2*(1 − exp(−T/tau2))
    let q = |t: f64| {
        params.a * params.tau1 * (1.0 - (-t / params.tau1).exp())
            + (i0 - params.a) * params.tau2 * (1.0 - (-t / params.tau2).exp())
    };

    // bisection to find T within at most 48 hours such that Q(T) = cv_charge_uas
    let t_high = 48.0 * 3600.0_f64;
    if q(t_high) < cv_charge_uas {
        return None;
    }

    let mut lo = 0.0_f64;
    let mut hi = t_high;
    for _ in 0..BISECTION_MAX_ITERS {
        let mid = (lo + hi) / 2.0;
        if q(mid) < cv_charge_uas {
            lo = mid;
        } else {
            hi = mid;
        }
        if hi - lo < BISECTION_TOLERANCE_SECS {
            let t_result = (lo + hi) / 2.0;
            log::debug!(
                "integral bisection converged: T = {:.0} s ({:.1} min)",
                t_result,
                t_result / 60.0,
            );
            match Duration::try_from_secs_f64(t_result) {
                Ok(d) => return Some(d),
                Err(e) => {
                    log::debug!(
                        "never mind, {e}; returning None (predict_cv_duration_from_integral)"
                    );
                    return None;
                }
            }
        }
    }

    log::debug!("cv model did not converge! returning original upper bound: {t_high} s");

    match Duration::try_from_secs_f64(t_high) {
        Ok(d) => Some(d),
        Err(e) => {
            log::debug!("never mind, {e}; returning None (predict_cv_duration_from_integral)");
            None
        }
    }
}
