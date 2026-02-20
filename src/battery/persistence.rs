use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::{model::RlsModel, predictor::BatteryPredictor};
use crate::battery::features::NUM_FEATURES;

/// Serialization format version. Increment when the format changes
/// incompatibly so that old files are gracefully discarded rather than
/// causing a deserialization error.
const STATE_VERSION: u32 = 0;

/// Serializable state for a single RLS model.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct RlsState {
    weights: Vec<f64>,
    p_matrix: Vec<f64>,
    lambda: f64,
    sample_count: u32,
}

impl RlsState {
    fn from_model(model: &RlsModel) -> Self {
        Self {
            weights: model.weights.clone(),
            p_matrix: model.p_matrix.clone(),
            lambda: model.lambda,
            sample_count: model.sample_count,
        }
    }

    fn to_model(&self) -> Option<RlsModel> {
        if self.weights.len() != NUM_FEATURES {
            log::warn!(
                "invalid rls weights length: expected {NUM_FEATURES}, got {}",
                self.weights.len()
            );
            return None;
        }
        if self.p_matrix.len() != NUM_FEATURES * NUM_FEATURES {
            log::warn!(
                "invalid rls p_matrix length: expected {}, got {}",
                NUM_FEATURES * NUM_FEATURES,
                self.p_matrix.len()
            );
            return None;
        }
        if !(0.0..=1.0).contains(&self.lambda) {
            log::warn!("invalid rls lambda: {}", self.lambda);
            return None;
        }

        Some(RlsModel {
            weights: self.weights.clone(),
            p_matrix: self.p_matrix.clone(),
            lambda: self.lambda,
            sample_count: self.sample_count,
        })
    }
}

/// Serializable state for BatteryPredictor.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PredictorState {
    /// Format version -- used to discard incompatible saved states.
    version: u32,
    rls_discharge: RlsState,
    rls_charge: RlsState,
    ewma_power_discharge: Option<f64>,
    ewma_power_charge: Option<f64>,
    ewma_alpha: f64,
    ewma_voltage: Option<f64>,
}

impl PredictorState {
    fn from_predictor(predictor: &BatteryPredictor) -> Self {
        Self {
            version: STATE_VERSION,
            rls_discharge: RlsState::from_model(&predictor.rls_discharge),
            rls_charge: RlsState::from_model(&predictor.rls_charge),
            ewma_power_discharge: predictor.ewma_power_discharge,
            ewma_power_charge: predictor.ewma_power_charge,
            ewma_alpha: predictor.ewma_alpha,
            ewma_voltage: predictor.ewma_voltage,
        }
    }

    fn to_predictor(&self) -> Option<BatteryPredictor> {
        if self.version != STATE_VERSION {
            log::info!(
                "battery predictor state version mismatch (got {}, want {}), starting fresh",
                self.version,
                STATE_VERSION
            );
            return None;
        }

        if !(0.0..=1.0).contains(&self.ewma_alpha) {
            log::warn!("invalid ewma_alpha: {}", self.ewma_alpha);
            return None;
        }

        let rls_discharge = self.rls_discharge.to_model()?;
        let rls_charge = self.rls_charge.to_model()?;

        Some(BatteryPredictor {
            rls_discharge,
            rls_charge,
            ewma_power_discharge: self.ewma_power_discharge,
            ewma_power_charge: self.ewma_power_charge,
            ewma_alpha: self.ewma_alpha,
            ewma_voltage: self.ewma_voltage,
        })
    }
}

/// Get the path to the predictor state file.
fn get_state_path() -> Result<PathBuf> {
    let state_dir = dirs::state_dir()
        .or_else(dirs::data_local_dir)
        .context("couldn't find state directory")?;

    let cadenza_state = state_dir.join("cadenza-shell");
    fs::create_dir_all(&cadenza_state)?;

    Ok(cadenza_state.join("battery_predictor.json"))
}

/// Save predictor state to disk.
pub fn save_predictor(predictor: &BatteryPredictor) -> Result<()> {
    let state = PredictorState::from_predictor(predictor);
    let json = serde_json::to_string_pretty(&state)?;

    let path = get_state_path()?;
    fs::write(&path, json).context("couldn't write predictor state")?;

    log::debug!("saved battery predictor state to {:?}", path);
    Ok(())
}

/// Load predictor state from disk.
///
/// Returns a fresh predictor if the file is missing, corrupt, or from
/// an incompatible version.
pub fn load_predictor() -> Result<BatteryPredictor> {
    let path = get_state_path()?;
    let json = fs::read_to_string(&path).context("couldn't read predictor state")?;
    let state: PredictorState = serde_json::from_str(&json)?;

    state.to_predictor().ok_or_else(|| {
        anyhow::anyhow!("predictor state was invalid or incompatible, starting fresh")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_roundtrip() {
        let predictor = BatteryPredictor::new();

        let state = PredictorState::from_predictor(&predictor);
        let restored = state.to_predictor().expect("roundtrip should succeed");

        assert_eq!(
            restored.rls_discharge.sample_count,
            predictor.rls_discharge.sample_count
        );
        assert_eq!(
            restored.rls_charge.sample_count,
            predictor.rls_charge.sample_count
        );
        assert_eq!(
            restored.ewma_power_discharge,
            predictor.ewma_power_discharge
        );
        assert_eq!(restored.ewma_power_charge, predictor.ewma_power_charge);
        assert_eq!(restored.ewma_alpha, predictor.ewma_alpha);
    }

    #[test]
    fn test_serialization_roundtrip() {
        let predictor = BatteryPredictor::new();
        let state = PredictorState::from_predictor(&predictor);

        let json = serde_json::to_string(&state).unwrap();
        assert!(!json.is_empty());

        let restored: PredictorState = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.version, STATE_VERSION);
        assert_eq!(
            restored.rls_discharge.sample_count,
            state.rls_discharge.sample_count
        );
    }

    #[test]
    fn test_version_mismatch_returns_none() {
        let predictor = BatteryPredictor::new();
        let mut state = PredictorState::from_predictor(&predictor);
        state.version = 99; // wrong version

        assert!(state.to_predictor().is_none());
    }

    #[test]
    fn test_invalid_ewma_alpha_returns_none() {
        let predictor = BatteryPredictor::new();
        let mut state = PredictorState::from_predictor(&predictor);
        state.ewma_alpha = 1.5; // out of range

        assert!(state.to_predictor().is_none());
    }
}
