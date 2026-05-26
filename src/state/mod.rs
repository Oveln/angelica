use serde::Serialize;
use std::path::Path;

use crate::config::FatigueConfig;
use crate::fatigue::{FatigueModel, FatiguePersist};

#[derive(Serialize, Clone, Debug)]
pub struct AgentState {
    pub fatigue: FatigueModel,
    pub woke_at: String,
    pub sleep_started_at: Option<String>,
    pub dream: Option<String>,
}

#[derive(serde::Deserialize)]
struct AgentStateRaw {
    fatigue: FatiguePersist,
    woke_at: String,
    sleep_started_at: Option<String>,
    dream: Option<String>,
}

impl<'de> serde::Deserialize<'de> for AgentState {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let raw = AgentStateRaw::deserialize(d)?;
        Ok(AgentState {
            fatigue: FatigueModel::from_persist(&raw.fatigue),
            woke_at: raw.woke_at,
            sleep_started_at: raw.sleep_started_at,
            dream: raw.dream,
        })
    }
}

impl AgentState {
    pub fn new(fatigue_config: &FatigueConfig) -> Self {
        Self {
            fatigue: FatigueModel::new(fatigue_config),
            woke_at: chrono::Local::now().to_rfc3339(),
            sleep_started_at: None,
            dream: None,
        }
    }

    pub fn load(path: &Path, fatigue_config: &FatigueConfig) -> anyhow::Result<Self> {
        let raw = std::fs::read_to_string(path)?;
        let deserialized: AgentState = serde_json::from_str(&raw)?;
        let mut fatigue = FatigueModel::new(fatigue_config);
        fatigue.fatigue = deserialized.fatigue.fatigue();
        fatigue.turns = deserialized.fatigue.turns();
        fatigue.tool_calls = deserialized.fatigue.tool_calls();
        Ok(AgentState {
            fatigue,
            ..deserialized
        })
    }

    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_fatigue_config() -> FatigueConfig {
        FatigueConfig {
            max_context_tokens: 100_000,
            curve_exponent: 1.5,
            sleep_threshold: 0.85,
            can_sleep_threshold: 0.6,
            groggy_turns: 3,
        }
    }

    #[test]
    fn roundtrip_serialization() {
        let config = test_fatigue_config();
        let state = AgentState::new(&config);
        let json = serde_json::to_string(&state).unwrap();
        let restored: AgentState = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.fatigue.fatigue(), 0.0);
    }

    #[test]
    fn fatigue_precision_preserved() {
        let config = test_fatigue_config();
        let mut state = AgentState::new(&config);
        state.fatigue.on_turn();
        let fatigue_val = state.fatigue.fatigue();
        let json = serde_json::to_string(&state).unwrap();
        let restored: AgentState = serde_json::from_str(&json).unwrap();
        assert!((restored.fatigue.fatigue() - fatigue_val).abs() < 1e-4);
    }

    #[test]
    fn save_and_load() {
        let config = test_fatigue_config();
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("state.json");
        let state = AgentState::new(&config);
        state.save(&path).unwrap();
        let loaded = AgentState::load(&path, &config).unwrap();
        assert_eq!(loaded.fatigue.fatigue(), state.fatigue.fatigue());
    }
}
