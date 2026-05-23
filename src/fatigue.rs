use serde::{Deserialize, Serialize};

use crate::config::FatigueConfig;

/// Fatigue derived directly from context window usage.
///
///   fatigue = (prompt_tokens / max_context_tokens) ^ (exponent + 1)
///
/// The power curve guarantees fatigue = 1.0 exactly when context hits the limit.
/// Early in the conversation fatigue grows slowly; near the limit it accelerates.
#[derive(Debug, Clone)]
pub struct FatigueModel {
    pub(crate) fatigue: f64,
    pub(crate) turns: u32,
    pub(crate) tool_calls: u32,
    pub(crate) groggy_turns: u32,
    pub(crate) max_context_tokens: u64,
    pub(crate) curve_exponent: f64,
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
            groggy_turns: 0,
            max_context_tokens: config.max_context_tokens.max(1),
            curve_exponent: config.curve_exponent.max(1.0),
            sleep_threshold: config.sleep_threshold,
            can_sleep_threshold: config.can_sleep_threshold,
            groggy_turns_on_wake: config.groggy_turns,
        }
    }

    pub fn update_from_context(&mut self, total_tokens: u64) {
        let ratio = (total_tokens as f64 / self.max_context_tokens as f64).min(1.0);
        tracing::info!(
            "Context tokens: {}, fatigue ratio: {:.3}",
            total_tokens, ratio
        );
        self.fatigue = ratio.powf(self.curve_exponent + 1.0);
    }

    pub fn on_turn(&mut self) {
        self.turns += 1;
        if self.groggy_turns > 0 {
            self.groggy_turns -= 1;
        }
    }

    pub fn add_tool_calls(&mut self, count: u32) {
        self.tool_calls += count;
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
            groggy_turns: 0,
            max_context_tokens: 120_000,
            curve_exponent: 1.0,
            sleep_threshold: 0.85,
            can_sleep_threshold: 0.6,
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
            max_context_tokens: 100_000,
            curve_exponent: 1.0,
            sleep_threshold: 0.85,
            can_sleep_threshold: 0.6,
            groggy_turns: 3,
        }
    }

    #[test]
    fn fatigue_from_context() {
        let config = test_fatigue_config();
        let mut model = FatigueModel::new(&config);
        // ratio=0.5, power=2, fatigue=0.25
        model.update_from_context(50_000);
        assert!((model.fatigue() - 0.25).abs() < 1e-6);
    }

    #[test]
    fn fatigue_reaches_one_at_max() {
        let config = test_fatigue_config();
        let mut model = FatigueModel::new(&config);
        model.update_from_context(100_000);
        assert!((model.fatigue() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn fatigue_capped_at_one() {
        let config = test_fatigue_config();
        let mut model = FatigueModel::new(&config);
        model.update_from_context(150_000);
        assert_eq!(model.fatigue(), 1.0);
    }

    #[test]
    fn delta_increases_as_context_fills() {
        let config = test_fatigue_config();
        let mut model = FatigueModel::new(&config);

        // Equal 20% chunks → increasing delta between consecutive steps
        let mut deltas = Vec::new();
        let mut prev = 0.0f64;
        for tokens in [20_000, 40_000, 60_000, 80_000] {
            model.update_from_context(tokens);
            deltas.push(model.fatigue() - prev);
            prev = model.fatigue();
        }

        for i in 1..deltas.len() {
            assert!(
                deltas[i] > deltas[i - 1],
                "delta[{}]={} should > delta[{}]={}",
                i, deltas[i], i - 1, deltas[i - 1]
            );
        }
    }

    #[test]
    fn zero_tokens_zero_fatigue() {
        let config = test_fatigue_config();
        let mut model = FatigueModel::new(&config);
        model.update_from_context(0);
        assert_eq!(model.fatigue(), 0.0);
    }

    #[test]
    fn on_turn_and_tool_calls_dont_affect_fatigue() {
        let config = test_fatigue_config();
        let mut model = FatigueModel::new(&config);
        model.update_from_context(50_000);
        let f = model.fatigue();
        model.on_turn();
        model.add_tool_calls(3);
        assert!((model.fatigue() - f).abs() < 1e-10);
        assert_eq!(model.turns(), 1);
        assert_eq!(model.tool_calls(), 3);
    }

    #[test]
    fn on_wake_resets_everything() {
        let config = test_fatigue_config();
        let mut model = FatigueModel::new(&config);
        model.update_from_context(80_000);
        model.on_turn();
        model.add_tool_calls(2);
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
        model.on_turn();
        assert!(model.is_groggy());
        model.on_turn();
        assert!(model.is_groggy());
        model.on_turn();
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
        // ratio=0.5 → fatigue=0.25
        model.update_from_context(50_000);
        assert!(!model.should_sleep());
        assert!(!model.can_sleep());
        // ratio≈0.775 → fatigue≈0.6
        model.update_from_context(77_500);
        assert!(!model.should_sleep());
        assert!(model.can_sleep());
        // ratio≈0.922 → fatigue≈0.85
        model.update_from_context(92_200);
        assert!(model.should_sleep());
        assert!(model.can_sleep());
    }

    #[test]
    fn serde_roundtrip() {
        let config = test_fatigue_config();
        let mut model = FatigueModel::new(&config);
        model.update_from_context(60_000);
        model.on_turn();
        model.add_tool_calls(2);
        let json = serde_json::to_string(&model).unwrap();
        let restored: FatigueModel = serde_json::from_str(&json).unwrap();
        assert!((restored.fatigue() - model.fatigue()).abs() < 1e-4);
        assert_eq!(restored.turns(), model.turns());
        assert_eq!(restored.tool_calls(), model.tool_calls());
    }

    #[test]
    fn higher_exponent_more_acceleration() {
        let config = FatigueConfig {
            max_context_tokens: 100_000,
            curve_exponent: 2.0,
            ..test_fatigue_config()
        };
        let mut model = FatigueModel::new(&config);
        // ratio=0.5, power=3, fatigue=0.125
        model.update_from_context(50_000);
        assert!((model.fatigue() - 0.125).abs() < 1e-4);
        model.update_from_context(100_000);
        assert!((model.fatigue() - 1.0).abs() < 1e-4);
    }
}
