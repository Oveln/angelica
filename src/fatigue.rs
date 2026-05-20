use serde::{Deserialize, Serialize};

use crate::config::FatigueConfig;

#[derive(Debug, Clone)]
pub struct FatigueModel {
    pub(crate) fatigue: f64,
    pub(crate) turns: u32,
    pub(crate) tool_calls: u32,
    pub(crate) base_per_turn: f64,
    pub(crate) groggy_turns: u32,
    pub(crate) tool_call_divisor: f64,
    pub(crate) sleep_threshold: f64,
    pub(crate) can_sleep_threshold: f64,
    pub(crate) groggy_turns_on_wake: u32,
}

impl FatigueModel {
    pub fn new(config: &FatigueConfig) -> Self {
        Self {
            fatigue: 0.0,
            turns: 0,
            tool_calls: 0,
            base_per_turn: config.per_turn_base,
            groggy_turns: 0,
            tool_call_divisor: 1.0 / config.per_tool_call_ratio.max(0.001),
            sleep_threshold: config.sleep_threshold,
            can_sleep_threshold: config.can_sleep_threshold,
            groggy_turns_on_wake: config.groggy_turns,
        }
    }

    pub fn on_turn(&mut self) {
        self.turns += 1;
        self.fatigue = (self.fatigue + self.base_per_turn).min(1.0);
        if self.groggy_turns > 0 {
            self.groggy_turns -= 1;
        }
    }

    pub fn on_tool_call(&mut self) {
        self.tool_calls += 1;
        self.fatigue = (self.fatigue + self.base_per_turn / self.tool_call_divisor).min(1.0);
    }

    pub fn with_persisted(mut self, fatigue: f64, turns: u32, tool_calls: u32) -> Self {
        self.fatigue = fatigue;
        self.turns = turns;
        self.tool_calls = tool_calls;
        self
    }

    pub fn on_wake(&mut self) {
        self.fatigue = 0.0;
        self.turns = 0;
        self.tool_calls = 0;
        self.groggy_turns = self.groggy_turns_on_wake;
    }

    pub fn is_groggy(&self) -> bool {
        self.groggy_turns > 0
    }

    pub fn should_sleep(&self) -> bool {
        self.fatigue >= self.sleep_threshold
    }

    pub fn can_sleep(&self) -> bool {
        self.fatigue >= self.can_sleep_threshold
    }

    pub fn fatigue(&self) -> f64 {
        self.fatigue
    }

    pub fn turns(&self) -> u32 {
        self.turns
    }

    pub fn tool_calls(&self) -> u32 {
        self.tool_calls
    }

    pub fn describe(&self) -> &'static str {
        if self.is_groggy() {
            return "刚醒来，还有点迷糊。休息过了。";
        }
        match self.fatigue {
            f if f < 0.3 => "精神饱满。",
            f if f < 0.6 => "正常。",
            f if f < 0.8 => "微微疲惫。",
            f if f < 0.95 => "很累了。",
            _ => "困倦，撑不住了。",
        }
    }
}

// Only persist the meaningful fields; runtime fields come from config on load.
#[derive(Serialize, Deserialize)]
struct FatiguePersist {
    #[serde(
        serialize_with = "serialize_fatigue_f64",
        deserialize_with = "deserialize_fatigue_f64"
    )]
    fatigue: f64,
    turns: u32,
    tool_calls: u32,
}

fn serialize_fatigue_f64<S: serde::Serializer>(val: &f64, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_f64((*val * 10_000.0).round() / 10_000.0)
}

fn deserialize_fatigue_f64<'de, D: serde::Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    let val: f64 = f64::deserialize(d)?;
    Ok((val * 10_000.0).round() / 10_000.0)
}

impl Serialize for FatigueModel {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        FatiguePersist {
            fatigue: self.fatigue,
            turns: self.turns,
            tool_calls: self.tool_calls,
        }
        .serialize(s)
    }
}

impl<'de> Deserialize<'de> for FatigueModel {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let p = FatiguePersist::deserialize(d)?;
        Ok(FatigueModel {
            fatigue: p.fatigue,
            turns: p.turns,
            tool_calls: p.tool_calls,
            base_per_turn: 0.015,
            groggy_turns: 0,
            tool_call_divisor: 3.003,
            sleep_threshold: 1.0,
            can_sleep_threshold: 0.8,
            groggy_turns_on_wake: 3,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::FatigueConfig;

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
    fn fatigue_accumulates_on_turn() {
        let config = test_fatigue_config();
        let mut model = FatigueModel::new(&config);
        assert!((model.base_per_turn - 0.015).abs() < 1e-6);
        model.on_turn();
        assert!((model.fatigue() - 0.015).abs() < 1e-6);
    }

    #[test]
    fn fatigue_config_controls_base() {
        let config = FatigueConfig {
            per_turn_base: 0.03,
            ..test_fatigue_config()
        };
        let model = FatigueModel::new(&config);
        assert!((model.base_per_turn - 0.03).abs() < 1e-6);
    }

    #[test]
    fn fatigue_caps_at_one() {
        let config = test_fatigue_config();
        let mut model = FatigueModel::new(&config);
        model.fatigue = 0.999;
        model.on_turn();
        assert_eq!(model.fatigue(), 1.0);
    }

    #[test]
    fn on_wake_resets_fatigue() {
        let config = test_fatigue_config();
        let mut model = FatigueModel::new(&config);
        model.on_turn();
        model.on_turn();
        assert!(model.fatigue() > 0.0);
        model.on_wake();
        assert_eq!(model.fatigue(), 0.0);
        assert_eq!(model.turns(), 0);
        assert_eq!(model.tool_calls(), 0);
        assert_eq!(model.groggy_turns, 3);
    }

    #[test]
    fn groggy_counts_down() {
        let config = test_fatigue_config();
        let mut model = FatigueModel::new(&config);
        model.on_wake();
        assert!(model.is_groggy());
        model.on_turn(); // 3 -> 2
        assert!(model.is_groggy());
        model.on_turn(); // 2 -> 1
        assert!(model.is_groggy());
        model.on_turn(); // 1 -> 0
        assert!(!model.is_groggy());
    }

    #[test]
    fn describe_returns_correct_strings() {
        let config = test_fatigue_config();
        let mut model = FatigueModel::new(&config);
        model.on_wake();
        assert!(model.describe().contains("刚醒来"));
        model.groggy_turns = 0;
        assert_eq!(model.describe(), "精神饱满。");
        model.fatigue = 0.4;
        assert_eq!(model.describe(), "正常。");
        model.fatigue = 0.7;
        assert_eq!(model.describe(), "微微疲惫。");
        model.fatigue = 0.9;
        assert_eq!(model.describe(), "很累了。");
        model.fatigue = 0.97;
        assert_eq!(model.describe(), "困倦，撑不住了。");
    }

    #[test]
    fn should_sleep_and_can_sleep() {
        let config = test_fatigue_config();
        let mut model = FatigueModel::new(&config);
        assert!(!model.should_sleep());
        assert!(!model.can_sleep());
        model.fatigue = 0.8;
        assert!(!model.should_sleep());
        assert!(model.can_sleep());
        model.fatigue = 1.0;
        assert!(model.should_sleep());
        assert!(model.can_sleep());
    }

    #[test]
    fn configurable_thresholds() {
        let config = FatigueConfig {
            sleep_threshold: 0.5,
            can_sleep_threshold: 0.3,
            ..test_fatigue_config()
        };
        let mut model = FatigueModel::new(&config);
        model.fatigue = 0.4;
        assert!(!model.should_sleep());
        assert!(model.can_sleep());
        model.fatigue = 0.6;
        assert!(model.should_sleep());
    }

    #[test]
    fn serde_roundtrip() {
        let config = test_fatigue_config();
        let mut model = FatigueModel::new(&config);
        model.on_turn();
        model.on_tool_call();
        let json = serde_json::to_string(&model).unwrap();
        let restored: FatigueModel = serde_json::from_str(&json).unwrap();
        assert!((restored.fatigue() - model.fatigue()).abs() < 1e-4);
        assert_eq!(restored.turns(), model.turns());
        assert_eq!(restored.tool_calls(), model.tool_calls());
    }
}
