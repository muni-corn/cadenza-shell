/// Learning rate for EMA updates to [`ChargeProfile`] session parameters.
pub(crate) const SESSION_LEARNING_RATE: f64 = 0.2;

/// Learning rate for the I_cut EWMA update.
pub(crate) const I_CUT_LEARNING_RATE: f64 = 0.1;

/// Learning rate for tau prior EWMA updates.
pub(crate) const TAU_PRIOR_LEARNING_RATE: f64 = 0.15;

/// Cold-start I_cut fraction of full-charge capacity (`0.05C`).
pub(crate) const I_CUT_DEFAULT_C_RATE: f64 = 0.05;

/// Default fast time constant prior (seconds).
pub(crate) const DEFAULT_TAU1_SECS: f64 = 300.0;

/// Default slow time constant prior (seconds).
pub(crate) const DEFAULT_TAU2_SECS: f64 = 1_800.0;

/// Default amplitude ratio prior (A / I0).
pub(crate) const DEFAULT_AMPLITUDE_RATIO: f64 = 0.7;

/// Window sizes (in reading count) used for rolling-median current
/// calculations.
pub(crate) const ROLLING_WINDOWS: [usize; 6] = [5, 10, 15, 20, 25, 30];
