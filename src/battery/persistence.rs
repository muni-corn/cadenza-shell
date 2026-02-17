use std::{fs, path::PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::{model::RlsModel, predictor::BatteryPredictor, profile::UsageProfile};
use crate::battery::{model::NUM_FEATURES, profile::NUM_USAGE_PROFILE_SLOTS};

/// Serializable state for BatteryPredictor.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PredictorState {
    rls_weights: Vec<f64>,
    rls_p_matrix: Vec<f64>,
    rls_lambda: f64,
    rls_sample_count: u32,
    profile_slots: Vec<f64>,
    profile_counts: Vec<u32>,
    profile_alpha: f64,
    ewma_power: Option<f64>,
    ewma_alpha: f64,
}

impl PredictorState {
    fn from_predictor(predictor: &BatteryPredictor) -> Self {
        Self {
            rls_weights: predictor.rls_model.weights.clone(),
            rls_p_matrix: predictor.rls_model.p_matrix.clone(),
            rls_lambda: predictor.rls_model.lambda,
            rls_sample_count: predictor.rls_model.sample_count,
            profile_slots: predictor.usage_profile.slots.clone(),
            profile_counts: predictor.usage_profile.counts.clone(),
            profile_alpha: predictor.usage_profile.alpha,
            ewma_power: predictor.ewma_power,
            ewma_alpha: predictor.ewma_alpha,
        }
    }

    fn to_predictor(&self) -> Result<BatteryPredictor> {
        // validate RLS dimensions
        if self.rls_weights.len() != NUM_FEATURES {
            anyhow::bail!(
                "invalid rls_weights length: expected {NUM_FEATURES}, got {}",
                self.rls_weights.len()
            );
        }
        if self.rls_p_matrix.len() != NUM_FEATURES * NUM_FEATURES {
            anyhow::bail!(
                "invalid rls_p_matrix length: expected {}, got {}",
                NUM_FEATURES * NUM_FEATURES,
                self.rls_p_matrix.len()
            );
        }

        // validate profile dimensions
        if self.profile_slots.len() != NUM_USAGE_PROFILE_SLOTS {
            anyhow::bail!(
                "invalid profile_slots length: expected {NUM_USAGE_PROFILE_SLOTS}, got {}",
                self.profile_slots.len()
            );
        }
        if self.profile_counts.len() != NUM_USAGE_PROFILE_SLOTS {
            anyhow::bail!(
                "invalid profile_counts length: expected {NUM_USAGE_PROFILE_SLOTS}, got {}",
                self.profile_counts.len()
            );
        }

        // validate ranges
        if !(0.0..=1.0).contains(&self.rls_lambda) {
            anyhow::bail!(
                "invalid rls_lambda: expected 0.0-1.0, got {}",
                self.rls_lambda
            );
        }
        if !(0.0..=1.0).contains(&self.profile_alpha) {
            anyhow::bail!(
                "invalid profile_alpha: expected 0.0-1.0, got {}",
                self.profile_alpha
            );
        }
        if !(0.0..=1.0).contains(&self.ewma_alpha) {
            anyhow::bail!(
                "invalid ewma_alpha: expected 0.0-1.0, got {}",
                self.ewma_alpha
            );
        }

        Ok(BatteryPredictor {
            rls_model: RlsModel {
                weights: self.rls_weights.clone(),
                p_matrix: self.rls_p_matrix.clone(),
                lambda: self.rls_lambda,
                sample_count: self.rls_sample_count,
            },
            usage_profile: UsageProfile {
                slots: self.profile_slots.clone(),
                counts: self.profile_counts.clone(),
                alpha: self.profile_alpha,
            },
            ewma_power: self.ewma_power,
            ewma_alpha: self.ewma_alpha,
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
pub fn load_predictor() -> Result<BatteryPredictor> {
    let path = get_state_path()?;
    let json = fs::read_to_string(&path).context("couldn't read predictor state")?;
    let state: PredictorState = serde_json::from_str(&json)?;

    log::debug!("loaded battery predictor state from {:?}", path);
    state.to_predictor().context("invalid predictor state data")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_roundtrip() {
        let predictor = BatteryPredictor::new();

        // convert to state and back
        let state = PredictorState::from_predictor(&predictor);
        let restored = state.to_predictor().unwrap();

        // check that key fields match
        assert_eq!(
            restored.rls_model.sample_count,
            predictor.rls_model.sample_count
        );
        assert_eq!(restored.ewma_power, predictor.ewma_power);
        assert_eq!(
            restored.usage_profile.slots.len(),
            predictor.usage_profile.slots.len()
        );
    }

    #[test]
    fn test_serialization() {
        let predictor = BatteryPredictor::new();
        let state = PredictorState::from_predictor(&predictor);

        // should serialize without error
        let json = serde_json::to_string(&state).unwrap();
        assert!(!json.is_empty());

        // should deserialize back
        let restored: PredictorState = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.rls_sample_count, state.rls_sample_count);
    }
}
