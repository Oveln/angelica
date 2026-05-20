use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::config::FatigueConfig;
use crate::fatigue::FatigueModel;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AgentState {
    pub fatigue: FatigueModel,
    pub woke_at: String,
    pub sleep_started_at: Option<String>,
    pub dream: Option<String>,
    pub last_snapshot: Option<String>,
}

impl AgentState {
    pub fn new(fatigue_config: &FatigueConfig) -> Self {
        Self {
            fatigue: FatigueModel::new(fatigue_config),
            woke_at: chrono::Local::now().to_rfc3339(),
            sleep_started_at: None,
            dream: None,
            last_snapshot: None,
        }
    }

    pub fn load(path: &Path, fatigue_config: &FatigueConfig) -> anyhow::Result<Self> {
        let raw = std::fs::read_to_string(path)?;
        let persisted: Self = serde_json::from_str(&raw)?;
        let fatigue = FatigueModel::new(fatigue_config).with_persisted(
            persisted.fatigue.fatigue(),
            persisted.fatigue.turns(),
            persisted.fatigue.tool_calls(),
        );
        Ok(Self {
            fatigue,
            ..persisted
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
            per_turn_base: 0.015,
            per_tool_call_ratio: 0.333,
            sleep_threshold: 1.0,
            can_sleep_threshold: 0.8,
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
